//! User-space bootstrap stack and page mapping.

use crate::kernel::memory::{
    address::{PhysicalFrameStart, UserVirtualAddress, VirtAddr},
    frame_allocator::{BumpFrameAllocator, FrameRangeOwner},
    paging,
};
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
const USER_STACK_BASE: u64 = 0x0000_7fff_f000_0000;
const _: () = assert!(USER_BAD_POINTER_BASE == 0x0000_4000_0000_2000);
const _: () = assert!(UserVirtualAddress::new(USER_PROGRAM_BASE).is_some());
const _: () = assert!(UserVirtualAddress::new(USER_STACK_BASE).is_some());

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

/// Allocate and map a fixed-base user-space stack.
///
/// Returns the virtual address one byte past the mapped stack range.
///
/// # Panics
///
/// Panics if physical frames cannot be allocated or page-table mapping fails.
pub fn allocate_user_stack(
    frame_allocator: &mut BumpFrameAllocator,
    pages: u64,
) -> UserVirtualAddress {
    assert!(pages > 0, "user stack must contain at least one page");
    let physical_range = frame_allocator
        .allocate_frames_for(pages, FrameRangeOwner::UserStack)
        .unwrap_or_else(|| panic!("OOM: failed to allocate {pages} pages for user stack"));
    let stack_size = pages
        .checked_mul(PAGE_SIZE)
        .expect("user stack size overflowed");

    // SAFETY: The active level-4 page table is identity mapped by early paging,
    // and the provided allocator supplies page-table frames for missing levels.
    unsafe {
        map_user_range(
            frame_allocator,
            UserVirtualAddress::new(USER_STACK_BASE).expect("user stack base must be valid"),
            physical_range.start(),
            pages,
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE,
        );
    }

    let stack_top = USER_STACK_BASE
        .checked_add(stack_size)
        .expect("user stack top address overflowed");
    UserVirtualAddress::new(stack_top).expect("user stack top must be a valid user address")
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
    stack_top: UserVirtualAddress,
    arguments: &[&str],
    environment: &[&str],
) -> PreparedUserStack {
    let stack_top = stack_top.as_u64();
    assert!(
        stack_top > USER_STACK_BASE,
        "user stack top must be above the fixed user stack base"
    );
    let stack_size = usize::try_from(stack_top - USER_STACK_BASE)
        .expect("user stack size must fit in usize before preparing arguments");
    assert!(
        paging::is_user_range_mapped_writable(
            usize::try_from(USER_STACK_BASE).expect("user stack base must fit in usize"),
            stack_size,
        ),
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

    let mut stack_pointer = stack_top;
    let mut argument_pointers = [0_u64; MAX_USER_ENTRY_ARGUMENTS];
    let mut environment_pointers = [0_u64; MAX_USER_ENTRY_ENVIRONMENT];

    for index in (0..arguments.len()).rev() {
        argument_pointers[index] =
            push_c_string(&mut stack_pointer, USER_STACK_BASE, arguments[index]);
    }
    for index in (0..environment.len()).rev() {
        environment_pointers[index] =
            push_c_string(&mut stack_pointer, USER_STACK_BASE, environment[index]);
    }

    stack_pointer = align_down(stack_pointer, POINTER_SIZE);
    let environment_values_pointer = push_pointer_array(
        &mut stack_pointer,
        USER_STACK_BASE,
        &environment_pointers[..environment.len()],
    );
    let argument_values_pointer = push_pointer_array(
        &mut stack_pointer,
        USER_STACK_BASE,
        &argument_pointers[..arguments.len()],
    );
    stack_pointer = align_down(stack_pointer, USER_STACK_ALIGNMENT);

    PreparedUserStack {
        stack_pointer: UserVirtualAddress::new(stack_pointer)
            .expect("initial stack pointer must be a valid user address"),
        argument_count: u64::try_from(arguments.len())
            .expect("argument count must fit in user entry register"),
        argument_values_pointer: UserVirtualAddress::new(argument_values_pointer)
            .expect("argv pointer array must be a valid user address"),
        environment_values_pointer: UserVirtualAddress::new(environment_values_pointer)
            .expect("environment pointer array must be a valid user address"),
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
        virtual_address < USER_STACK_BASE,
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

fn push_c_string(stack_pointer: &mut u64, stack_base: u64, value: &str) -> u64 {
    let length = u64::try_from(value.len().saturating_add(1))
        .expect("user entry string length must fit in u64");
    *stack_pointer = stack_pointer
        .checked_sub(length)
        .expect("user entry string stack pointer underflowed");
    assert!(
        *stack_pointer >= stack_base,
        "user entry strings must fit in the mapped user stack"
    );

    let destination = usize::try_from(*stack_pointer)
        .expect("user entry string address must fit in usize") as *mut u8;
    // SAFETY: The destination range was carved out of the mapped user stack,
    // and the source string plus trailing NUL fits in that range.
    unsafe {
        core::ptr::copy_nonoverlapping(value.as_ptr(), destination, value.len());
        destination.add(value.len()).write(0);
    }

    *stack_pointer
}

fn push_pointer_array(stack_pointer: &mut u64, stack_base: u64, pointers: &[u64]) -> u64 {
    let entries = u64::try_from(pointers.len().saturating_add(1))
        .expect("user entry pointer count must fit in u64");
    let byte_count = entries
        .checked_mul(POINTER_SIZE)
        .expect("user entry pointer array size overflowed");
    *stack_pointer = stack_pointer
        .checked_sub(byte_count)
        .expect("user entry pointer array stack pointer underflowed");
    assert!(
        *stack_pointer >= stack_base,
        "user entry pointer arrays must fit in the mapped user stack"
    );

    for (index, pointer) in pointers.iter().enumerate() {
        let offset = u64::try_from(index)
            .expect("user entry pointer index must fit in u64")
            .checked_mul(POINTER_SIZE)
            .expect("user entry pointer offset overflowed");
        write_stack_u64(*stack_pointer + offset, *pointer);
    }
    let terminator_offset = u64::try_from(pointers.len())
        .expect("user entry pointer terminator index must fit in u64")
        .checked_mul(POINTER_SIZE)
        .expect("user entry pointer terminator offset overflowed");
    write_stack_u64(*stack_pointer + terminator_offset, 0);

    *stack_pointer
}

fn write_stack_u64(address: u64, value: u64) {
    let pointer =
        usize::try_from(address).expect("user stack pointer must fit in usize") as *mut u64;
    // SAFETY: The caller provides an address inside a writable mapped user
    // stack slot reserved for one 64-bit pointer value.
    unsafe {
        pointer.write(value);
    }
}

fn align_down(value: u64, alignment: u64) -> u64 {
    debug_assert!(alignment.is_power_of_two());
    value & !(alignment - 1)
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
