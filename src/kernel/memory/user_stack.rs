//! User-space bootstrap stack and page mapping.

use crate::kernel::memory::{frame_allocator::BumpFrameAllocator, paging};
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
/// Virtual base used by linked user demo executables.
pub const USER_PROGRAM_BASE: u64 = 0x0000_4000_0000_0000;
const USER_DATA_BASE: u64 = USER_PROGRAM_BASE + PAGE_SIZE;
const USER_BAD_POINTER_BASE: u64 = USER_DATA_BASE + PAGE_SIZE;
const USER_STACK_BASE: u64 = 0x0000_7fff_f000_0000;
const _: () = assert!(USER_BAD_POINTER_BASE == 0x0000_4000_0000_2000);

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
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE,
        );
    }

    USER_STACK_BASE
        .checked_add(stack_size)
        .expect("user stack top address overflowed")
}

/// Allocate one physical frame and map it at a page-aligned user virtual address.
///
/// Returns the identity-mapped physical address of the allocated frame.
///
/// # Panics
///
/// Panics if the address is not page-aligned, the address is outside user
/// space, a physical frame cannot be allocated, or page-table mapping fails.
pub fn allocate_and_map_user_page(
    frame_allocator: &mut BumpFrameAllocator,
    virtual_address: u64,
    flags: PageTableFlags,
) -> u64 {
    assert!(
        virtual_address.is_multiple_of(PAGE_SIZE),
        "user page virtual address must be 4KiB aligned"
    );
    assert!(
        virtual_address < USER_STACK_BASE,
        "user page virtual address must stay below the user stack"
    );
    let physical_address = frame_allocator
        .allocate_frame()
        .expect("OOM: failed to allocate user page");
    let page_pointer = physical_address as *mut u8;

    // SAFETY: `physical_address` is a freshly allocated identity-mapped frame.
    unsafe {
        core::ptr::write_bytes(page_pointer, 0, PAGE_SIZE_USIZE);
        map_user_range(frame_allocator, virtual_address, physical_address, 1, flags);
    }

    physical_address
}

/// Return whether the fixed user stack is writable and its guard page is unmapped.
///
/// # Panics
///
/// Panics if `pages` is zero or the stack size overflows.
pub fn verify_user_stack_mapping(pages: u64) -> bool {
    assert!(
        pages > 0,
        "user stack verification requires at least one page"
    );
    let stack_size = pages
        .checked_mul(PAGE_SIZE)
        .and_then(|bytes| usize::try_from(bytes).ok())
        .expect("user stack verification size overflowed");
    let guard_page = USER_STACK_BASE
        .checked_sub(PAGE_SIZE)
        .expect("user stack guard address underflowed");

    paging::is_user_range_mapped_writable(
        usize::try_from(USER_STACK_BASE).expect("user stack base must fit in usize"),
        stack_size,
    ) && !paging::is_user_range_mapped_readable(
        usize::try_from(guard_page).expect("user stack guard must fit in usize"),
        PAGE_SIZE_USIZE,
    )
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
                .expect("failed to map user page")
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
