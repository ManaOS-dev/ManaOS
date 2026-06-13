//! User-space bootstrap stack and page mapping.

use crate::kernel::memory::{
    address::{PhysicalFrameStart, UserVirtualAddress, VirtAddr},
    frame_allocator::{BumpFrameAllocator, FrameRangeOwner},
    paging,
};
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::{
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB,
    },
    PhysAddr as X86PhysAddr, VirtAddr as X86VirtAddr,
};

const PAGE_SIZE: u64 = 4096;
const PAGE_SIZE_USIZE: usize = 4096;
const POINTER_SIZE: u64 = core::mem::size_of::<u64>() as u64;
const USER_STACK_ALIGNMENT: u64 = 16;
const MAX_USER_ENTRY_ARGUMENTS: usize = 8;
const MAX_USER_ENTRY_ENVIRONMENT: usize = 8;
/// Virtual base used by linked user demo executables.
pub const USER_PROGRAM_BASE: u64 = 0x0000_4000_0000_0000;
const USER_DATA_BASE: u64 = USER_PROGRAM_BASE + PAGE_SIZE;
const USER_BAD_POINTER_BASE: u64 = USER_DATA_BASE + PAGE_SIZE;
const USER_STACK_REGION_BASE: u64 = 0x0000_7fff_f000_0000;
// Each stack slot reserves 1 MiB so small smoke stacks have an unmapped gap
// between writable stack ranges while per-process page tables are still absent.
const USER_STACK_SLOT_BYTES: u64 = 0x0010_0000;
static NEXT_USER_STACK_SLOT: AtomicU64 = AtomicU64::new(0);
const _: () = assert!(USER_BAD_POINTER_BASE == 0x0000_4000_0000_2000);
const _: () = assert!(UserVirtualAddress::new(USER_PROGRAM_BASE).is_some());
const _: () = assert!(UserVirtualAddress::new(USER_STACK_REGION_BASE).is_some());

/// Allocated user stack virtual range.
#[derive(Debug, Clone, Copy)]
pub struct AllocatedUserStack {
    base: UserVirtualAddress,
    top: UserVirtualAddress,
    page_count: u64,
}

impl AllocatedUserStack {
    /// Return the first writable byte in this user stack.
    pub fn base(&self) -> UserVirtualAddress {
        self.base
    }

    /// Return the user stack pointer one byte past the writable range.
    pub fn top(&self) -> UserVirtualAddress {
        self.top
    }

    /// Return the number of writable 4 KiB pages in this stack.
    pub fn page_count(&self) -> u64 {
        self.page_count
    }

    fn byte_len(self) -> u64 {
        self.page_count
            .checked_mul(PAGE_SIZE)
            .expect("user stack byte length overflowed")
    }
}

/// Prepared user stack metadata for first user-mode entry.
#[derive(Debug, Clone, Copy)]
pub struct PreparedUserStack {
    stack_pointer: UserVirtualAddress,
    argument_count: u64,
    argument_values_pointer: UserVirtualAddress,
    environment_values_pointer: UserVirtualAddress,
}

impl PreparedUserStack {
    /// Return the initial user stack pointer.
    pub fn stack_pointer(&self) -> UserVirtualAddress {
        self.stack_pointer
    }

    /// Return the number of entries in the user `argv` array.
    pub fn argument_count(&self) -> u64 {
        self.argument_count
    }

    /// Return the user virtual address of the `argv` pointer array.
    pub fn argument_values_pointer(&self) -> UserVirtualAddress {
        self.argument_values_pointer
    }

    /// Return the user virtual address of the environment pointer array.
    pub fn environment_values_pointer(&self) -> UserVirtualAddress {
        self.environment_values_pointer
    }
}

/// Allocate and map one slot-based user-space stack.
///
/// Returns the mapped stack range and top address.
///
/// # Panics
///
/// Panics if physical frames cannot be allocated or page-table mapping fails.
pub fn allocate_user_stack(
    frame_allocator: &mut BumpFrameAllocator,
    pages: u64,
) -> AllocatedUserStack {
    assert!(pages > 0, "user stack must contain at least one page");
    let physical_range = frame_allocator
        .allocate_frames_for(pages, FrameRangeOwner::UserStack)
        .unwrap_or_else(|| panic!("OOM: failed to allocate {pages} pages for user stack"));
    let stack_size = pages
        .checked_mul(PAGE_SIZE)
        .expect("user stack size overflowed");
    assert!(
        stack_size < USER_STACK_SLOT_BYTES - PAGE_SIZE,
        "user stack must fit inside one virtual stack slot"
    );
    let stack_slot = NEXT_USER_STACK_SLOT.fetch_add(1, Ordering::AcqRel);
    let stack_slot_offset = stack_slot
        .checked_mul(USER_STACK_SLOT_BYTES)
        .expect("user stack slot offset overflowed");
    let stack_base = USER_STACK_REGION_BASE
        .checked_add(stack_slot_offset)
        .expect("user stack base address overflowed");
    let stack_base =
        UserVirtualAddress::new(stack_base).expect("user stack base must be a valid user address");
    let stack_top = stack_base
        .checked_add(stack_size)
        .expect("user stack top address overflowed");

    // SAFETY: The active level-4 page table is identity mapped by early paging,
    // and the provided allocator supplies page-table frames for missing levels.
    unsafe {
        map_user_range(
            frame_allocator,
            stack_base,
            physical_range.start(),
            pages,
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE,
        );
    }

    AllocatedUserStack {
        base: stack_base,
        top: stack_top,
        page_count: pages,
    }
}

/// Place `argv` and environment strings on the mapped user stack.
///
/// Returns the adjusted stack pointer and user virtual addresses for the
/// null-terminated pointer arrays.
///
/// # Panics
///
/// Panics if the stack is invalid, the fixed argument limits are exceeded, or
/// the argument block does not fit in the mapped stack range.
pub fn prepare_initial_stack(
    stack: AllocatedUserStack,
    arguments: &[&str],
    environment: &[&str],
) -> PreparedUserStack {
    let stack_top_address = stack.top();
    let stack_top = stack_top_address.as_u64();
    let stack_base = stack.base();
    assert!(
        stack_top > stack_base.as_u64(),
        "user stack top must be above the allocated stack base"
    );
    let stack_size = usize::try_from(stack.byte_len())
        .expect("user stack size must fit in usize before preparing arguments");
    assert!(
        paging::is_user_range_mapped_writable(stack_base.as_usize(), stack_size,),
        "user entry arguments require a mapped writable user stack"
    );
    assert!(
        arguments.len() <= MAX_USER_ENTRY_ARGUMENTS,
        "too many user entry arguments"
    );
    assert!(
        environment.len() <= MAX_USER_ENTRY_ENVIRONMENT,
        "too many user entry environment entries"
    );

    let mut stack_cursor = UserStackCursor::new(stack_top_address, stack_base);
    let mut argument_pointers = [None; MAX_USER_ENTRY_ARGUMENTS];
    let mut environment_pointers = [None; MAX_USER_ENTRY_ENVIRONMENT];

    for index in (0..arguments.len()).rev() {
        argument_pointers[index] = Some(push_c_string(&mut stack_cursor, arguments[index]));
    }
    for index in (0..environment.len()).rev() {
        environment_pointers[index] = Some(push_c_string(&mut stack_cursor, environment[index]));
    }

    stack_cursor.align_down(POINTER_SIZE);
    let environment_values_pointer = push_pointer_array(
        &mut stack_cursor,
        &environment_pointers[..environment.len()],
    );
    let argument_values_pointer =
        push_pointer_array(&mut stack_cursor, &argument_pointers[..arguments.len()]);
    stack_cursor.align_down(USER_STACK_ALIGNMENT);

    PreparedUserStack {
        stack_pointer: stack_cursor.into_user_address(),
        argument_count: u64::try_from(arguments.len())
            .expect("argument count must fit in user entry register"),
        argument_values_pointer,
        environment_values_pointer,
    }
}

/// Allocate one physical frame and map it at a page-aligned user virtual address.
///
/// Returns the allocated physical frame start.
///
/// # Panics
///
/// Panics if the address is not page-aligned, the address is outside user
/// space, a physical frame cannot be allocated, or page-table mapping fails.
pub fn allocate_and_map_user_page(
    frame_allocator: &mut BumpFrameAllocator,
    virtual_address: UserVirtualAddress,
    flags: PageTableFlags,
    owner: FrameRangeOwner,
) -> PhysicalFrameStart {
    let virtual_address = virtual_address.as_u64();
    assert!(
        virtual_address.is_multiple_of(PAGE_SIZE),
        "user page virtual address must be 4KiB aligned"
    );
    assert!(
        virtual_address < USER_STACK_REGION_BASE,
        "user page virtual address must stay below the user stack"
    );
    let physical_address = frame_allocator
        .allocate_frame_for(owner)
        .expect("OOM: failed to allocate user page");
    let page_pointer = physical_address.as_usize() as *mut u8;

    // SAFETY: `physical_address` is a freshly allocated identity-mapped frame.
    unsafe {
        core::ptr::write_bytes(page_pointer, 0, PAGE_SIZE_USIZE);
        map_user_range(
            frame_allocator,
            UserVirtualAddress::new(virtual_address)
                .expect("validated user page address must remain valid"),
            physical_address,
            1,
            flags,
        );
    }

    physical_address
}

/// Return whether the user stack is writable and its guard page is unmapped.
///
/// # Panics
///
/// Panics if the stack has no pages or the stack size overflows.
pub fn verify_user_stack_mapping(stack: AllocatedUserStack) -> bool {
    assert!(
        stack.page_count() > 0,
        "user stack verification requires at least one page"
    );
    let stack_size =
        usize::try_from(stack.byte_len()).expect("user stack verification size overflowed");
    let guard_page = stack
        .base()
        .as_u64()
        .checked_sub(PAGE_SIZE)
        .expect("user stack guard address underflowed");

    paging::is_user_range_mapped_writable(stack.base().as_usize(), stack_size)
        && !paging::is_user_range_mapped_readable(
            usize::try_from(guard_page).expect("user stack guard must fit in usize"),
            PAGE_SIZE_USIZE,
        )
}

struct UserStackCursor {
    pointer: UserVirtualAddress,
    base: UserVirtualAddress,
}

impl UserStackCursor {
    fn new(pointer: UserVirtualAddress, base: UserVirtualAddress) -> Self {
        assert!(
            pointer.as_u64() > base.as_u64(),
            "user stack cursor must start above the stack base"
        );
        Self { pointer, base }
    }

    fn push_bytes(&mut self, byte_count: u64) -> UserVirtualAddress {
        let next_pointer = self
            .pointer
            .checked_sub(byte_count)
            .expect("user stack cursor underflowed");
        assert!(
            next_pointer.as_u64() >= self.base.as_u64(),
            "user entry data must fit in the mapped user stack"
        );
        self.pointer = next_pointer;
        self.pointer
    }

    fn align_down(&mut self, alignment: u64) {
        debug_assert!(alignment.is_power_of_two());
        let aligned_pointer = self.pointer.as_u64() & !(alignment - 1);
        let aligned_pointer = UserVirtualAddress::new(aligned_pointer)
            .expect("aligned user stack pointer must remain valid");
        assert!(
            aligned_pointer.as_u64() >= self.base.as_u64(),
            "aligned user stack pointer must stay inside the mapped user stack"
        );
        self.pointer = aligned_pointer;
    }

    fn into_user_address(self) -> UserVirtualAddress {
        self.pointer
    }
}

fn push_c_string(stack_cursor: &mut UserStackCursor, value: &str) -> UserVirtualAddress {
    let length = u64::try_from(value.len().saturating_add(1))
        .expect("user entry string length must fit in u64");
    let stack_pointer = stack_cursor.push_bytes(length);

    let destination = stack_pointer.as_usize() as *mut u8;
    // SAFETY: The destination range was carved out of the mapped user stack,
    // and the source string plus trailing NUL fits in that range.
    unsafe {
        core::ptr::copy_nonoverlapping(value.as_ptr(), destination, value.len());
        destination.add(value.len()).write(0);
    }

    stack_pointer
}

fn push_pointer_array(
    stack_cursor: &mut UserStackCursor,
    pointers: &[Option<UserVirtualAddress>],
) -> UserVirtualAddress {
    let entries = u64::try_from(pointers.len().saturating_add(1))
        .expect("user entry pointer count must fit in u64");
    let byte_count = entries
        .checked_mul(POINTER_SIZE)
        .expect("user entry pointer array size overflowed");
    let stack_pointer = stack_cursor.push_bytes(byte_count);

    for (index, pointer) in pointers.iter().enumerate() {
        let offset = u64::try_from(index)
            .expect("user entry pointer index must fit in u64")
            .checked_mul(POINTER_SIZE)
            .expect("user entry pointer offset overflowed");
        let slot_address = stack_pointer
            .checked_add(offset)
            .expect("user entry pointer slot address overflowed");
        let pointer = pointer
            .expect("user entry pointer array slot must be initialized")
            .as_u64();
        write_stack_u64(slot_address, pointer);
    }
    let terminator_offset = u64::try_from(pointers.len())
        .expect("user entry pointer terminator index must fit in u64")
        .checked_mul(POINTER_SIZE)
        .expect("user entry pointer terminator offset overflowed");
    let terminator_address = stack_pointer
        .checked_add(terminator_offset)
        .expect("user entry pointer terminator address overflowed");
    write_stack_u64(terminator_address, 0);

    stack_pointer
}

fn write_stack_u64(address: UserVirtualAddress, value: u64) {
    let pointer = address.as_usize() as *mut u64;
    // SAFETY: The caller provides an address inside a writable mapped user
    // stack slot reserved for one 64-bit pointer value.
    unsafe {
        pointer.write(value);
    }
}

unsafe fn map_user_range(
    frame_allocator: &mut BumpFrameAllocator,
    virtual_start: UserVirtualAddress,
    physical_start: PhysicalFrameStart,
    pages: u64,
    flags: PageTableFlags,
) {
    let (level_4_frame, _) = Cr3::read();
    let level_4_table = level_4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: The active page table is identity mapped, so its physical address
    // is a valid virtual address in the current address space.
    let level_4_table = unsafe { &mut *level_4_table };
    // SAFETY: ManaOS uses identity-mapped physical memory for page-table access.
    let mut mapper = unsafe { OffsetPageTable::new(level_4_table, X86VirtAddr::new(0)) };
    let mut wrapper = UserFrameAllocator { frame_allocator };
    let virtual_start = VirtAddr::new(virtual_start.as_u64());
    let physical_start = physical_start.as_address();

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
        let page = Page::<Size4KiB>::containing_address(X86VirtAddr::new(virtual_address.as_u64()));
        let frame = PhysFrame::containing_address(X86PhysAddr::new(physical_address.as_u64()));

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
            .allocate_frame_for(FrameRangeOwner::PageTable)
            .map(|address| PhysFrame::containing_address(X86PhysAddr::new(address.as_u64())))
    }
}
