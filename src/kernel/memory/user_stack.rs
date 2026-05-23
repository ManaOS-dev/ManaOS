//! User-space bootstrap stack and code mapping.

use crate::kernel::memory::frame_allocator::BumpFrameAllocator;
use x86_64::{
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB,
    },
    PhysAddr, VirtAddr,
};

const PAGE_SIZE: u64 = 4096;
const PAGE_SIZE_USIZE: usize = 4096;
const USER_PROGRAM_BASE: u64 = 0x0000_4000_0000_0000;
const USER_STACK_BASE: u64 = 0x0000_7fff_f000_0000;
const USER_FILE_DEMO_PROGRAM: &[u8] = &[
    0xb8, 0x03, 0x00, 0x00, 0x00, // mov eax, SYS_OPEN
    0x48, 0x8d, 0x3d, 0x44, 0x00, 0x00, 0x00, // lea rdi, [rip + PATH]
    0x0f, 0x05, // syscall
    0x89, 0xc3, // mov ebx, eax
    0xb8, 0x05, 0x00, 0x00, 0x00, // mov eax, SYS_READ
    0x89, 0xdf, // mov edi, ebx
    0x48, 0x8d, 0x35, 0x49, 0x00, 0x00, 0x00, // lea rsi, [rip + BUFFER]
    0xba, 0x40, 0x00, 0x00, 0x00, // mov edx, BUFFER_LEN
    0x0f, 0x05, // syscall
    0x89, 0xc5, // mov ebp, eax
    0xb8, 0x04, 0x00, 0x00, 0x00, // mov eax, SYS_CLOSE
    0x89, 0xdf, // mov edi, ebx
    0x0f, 0x05, // syscall
    0xb8, 0x01, 0x00, 0x00, 0x00, // mov eax, SYS_WRITE
    0xbf, 0x01, 0x00, 0x00, 0x00, // mov edi, STDOUT
    0x48, 0x8d, 0x35, 0x26, 0x00, 0x00, 0x00, // lea rsi, [rip + BUFFER]
    0x89, 0xea, // mov edx, ebp
    0x0f, 0x05, // syscall
    0xb8, 0x02, 0x00, 0x00, 0x00, // mov eax, SYS_EXIT
    0x31, 0xff, // xor edi, edi
    0x0f, 0x05, // syscall
    0xeb, 0xfe, // jump to self if SYS_EXIT returns unexpectedly
    b'/', b'h', b'e', b'l', b'l', b'o', b'.', b't', b'x', b't', 0x00, 0x90, 0x90, 0x90, 0x90, 0x90,
    0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, // BUFFER starts here.
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Allocate and map a fixed-base user-space stack.
///
/// Returns the virtual address one byte past the mapped stack range.
///
/// # Panics
///
/// Panics if physical frames cannot be allocated or page-table mapping fails.
pub fn allocate_user_stack(frame_allocator: &mut BumpFrameAllocator, pages: u64) -> u64 {
    assert!(pages > 0, "user stack must contain at least one page");
    let physical_start = frame_allocator
        .allocate_frames(pages)
        .unwrap_or_else(|| panic!("OOM: failed to allocate {pages} pages for user stack"));
    let stack_size = pages
        .checked_mul(PAGE_SIZE)
        .expect("user stack size overflowed");

    // SAFETY: The active level-4 page table is identity mapped by early paging,
    // and the provided allocator supplies page-table frames for missing levels.
    unsafe {
        map_user_range(
            frame_allocator,
            USER_STACK_BASE,
            physical_start,
            pages,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
        );
    }

    USER_STACK_BASE
        .checked_add(stack_size)
        .expect("user stack top address overflowed")
}

/// Allocate and map the built-in user-space file syscall demo program.
///
/// Returns the virtual entry point of the mapped program.
///
/// # Panics
///
/// Panics if a physical frame cannot be allocated or the program page cannot be
/// mapped.
pub fn allocate_user_file_demo(frame_allocator: &mut BumpFrameAllocator) -> u64 {
    assert!(
        USER_FILE_DEMO_PROGRAM.len() <= PAGE_SIZE_USIZE,
        "built-in user program must fit in one page"
    );

    let physical_start = frame_allocator
        .allocate_frame()
        .expect("OOM: failed to allocate built-in user program page");
    let program_page = physical_start as *mut u8;

    // SAFETY: `physical_start` is a freshly allocated identity-mapped frame.
    // The writes initialize the whole page before the user mapping is installed.
    unsafe {
        core::ptr::write_bytes(program_page, 0, PAGE_SIZE_USIZE);
        core::ptr::copy_nonoverlapping(
            USER_FILE_DEMO_PROGRAM.as_ptr(),
            program_page,
            USER_FILE_DEMO_PROGRAM.len(),
        );
        map_user_range(
            frame_allocator,
            USER_PROGRAM_BASE,
            physical_start,
            1,
            PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE,
        );
    }

    USER_PROGRAM_BASE
}

unsafe fn map_user_range(
    frame_allocator: &mut BumpFrameAllocator,
    virtual_start: u64,
    physical_start: u64,
    pages: u64,
    flags: PageTableFlags,
) {
    let (level_4_frame, _) = Cr3::read();
    let level_4_table = level_4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: The active page table is identity mapped, so its physical address
    // is a valid virtual address in the current address space.
    let level_4_table = unsafe { &mut *level_4_table };
    // SAFETY: ManaOS uses identity-mapped physical memory for page-table access.
    let mut mapper = unsafe { OffsetPageTable::new(level_4_table, VirtAddr::new(0)) };
    let mut wrapper = UserFrameAllocator { frame_allocator };

    for index in 0..pages {
        let offset = index
            .checked_mul(PAGE_SIZE)
            .expect("user mapping offset overflowed");
        let virtual_address = virtual_start
            .checked_add(offset)
            .expect("user virtual address overflowed");
        let physical_address = physical_start
            .checked_add(offset)
            .expect("user physical address overflowed");
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virtual_address));
        let frame = PhysFrame::containing_address(PhysAddr::new(physical_address));

        // SAFETY: `frame` is owned by the caller for this range, `page` is in
        // the fixed user stack range, and `wrapper` allocates new page-table
        // frames when the mapper needs them.
        unsafe {
            mapper
                .map_to(page, frame, flags, &mut wrapper)
                .expect("failed to map user stack page")
                .flush();
        }
    }
}

struct UserFrameAllocator<'a> {
    frame_allocator: &'a mut BumpFrameAllocator,
}

// SAFETY: UserFrameAllocator delegates to BumpFrameAllocator, which returns each
// frame at most once from registered conventional memory regions.
unsafe impl FrameAllocator<Size4KiB> for UserFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.frame_allocator
            .allocate_frame()
            .map(|address| PhysFrame::containing_address(PhysAddr::new(address)))
    }
}
