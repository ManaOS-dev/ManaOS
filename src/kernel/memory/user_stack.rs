//! User-space bootstrap stack and page mapping.

use crate::kernel::memory::{
    address::{
        FrameCount, PhysicalFrameRange, PhysicalFrameStart, UserPageStart, UserVirtualAddress,
        VirtAddr,
    },
    address_space::UserAddressSpace,
    frame_allocator::{FrameRangeOwner, PhysicalFrameAllocator},
    user_layout::{USER_STACK_REGION_BASE, USER_STACK_SLOT_BYTES},
};
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::paging::PageTableFlags;

const PAGE_SIZE: u64 = 4096;
const PAGE_SIZE_USIZE: usize = 4096;
const POINTER_SIZE: u64 = core::mem::size_of::<u64>() as u64;
const USER_STACK_ALIGNMENT: u64 = 16;
const MAX_USER_ENTRY_ARGUMENTS: usize = 8;
const MAX_USER_ENTRY_ENVIRONMENT: usize = 8;
static NEXT_USER_STACK_SLOT: AtomicU64 = AtomicU64::new(0);

/// Allocated user stack virtual range.
#[derive(Debug, Clone, Copy)]
pub struct AllocatedUserStack {
    base: UserVirtualAddress,
    top: UserVirtualAddress,
    physical_range: PhysicalFrameRange,
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

    /// Return the physical frames backing this user stack.
    pub fn physical_range(&self) -> PhysicalFrameRange {
        self.physical_range
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
    address_space: UserAddressSpace,
    frame_allocator: &mut PhysicalFrameAllocator,
    pages: u64,
) -> AllocatedUserStack {
    assert!(pages > 0, "user stack must contain at least one page");
    let frame_count = FrameCount::new(pages).expect("user stack frame count must be valid");
    let physical_range = frame_allocator
        .allocate_frames_for(frame_count, FrameRangeOwner::UserStack)
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
    let stack_base = UserVirtualAddress::new(VirtAddr::new(stack_base))
        .expect("user stack base must be a valid user address");
    let stack_base_page =
        UserPageStart::new(stack_base).expect("user stack base must be page-aligned");
    let stack_top = stack_base
        .checked_add(stack_size)
        .expect("user stack top address overflowed");

    for index in 0..pages {
        let offset = index
            .checked_mul(PAGE_SIZE)
            .expect("user stack mapping offset overflowed");
        let virtual_page_start = stack_base_page
            .checked_add(offset)
            .expect("user stack virtual address overflowed");
        let physical_start = PhysicalFrameStart::new(
            physical_range
                .start()
                .as_address()
                .checked_add(offset)
                .expect("user stack physical address overflowed"),
        )
        .expect("user stack physical page must remain aligned");
        address_space.map_user_page(
            frame_allocator,
            virtual_page_start,
            physical_start,
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE,
        );
    }

    AllocatedUserStack {
        base: stack_base,
        top: stack_top,
        physical_range,
        page_count: pages,
    }
}

/// Place byte-preserving `argv` and environment strings on the mapped user stack.
///
/// Returns the adjusted stack pointer and user virtual addresses for the
/// null-terminated pointer arrays.
///
/// # Panics
///
/// Panics if the stack is invalid, the fixed argument limits are exceeded, any
/// value contains an interior NUL byte, or the argument block does not fit in
/// the mapped stack range.
pub fn prepare_initial_stack_bytes(
    address_space: UserAddressSpace,
    stack: AllocatedUserStack,
    arguments: &[&[u8]],
    environment: &[&[u8]],
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
        address_space.is_user_range_mapped_writable(stack_base.as_usize(), stack_size,),
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
    assert!(
        arguments
            .iter()
            .chain(environment.iter())
            .all(|value| !value.contains(&0)),
        "user entry strings must not contain interior NUL bytes"
    );

    let mut stack_cursor = UserStackCursor::new(stack, stack_top_address);
    let mut argument_pointers = [None; MAX_USER_ENTRY_ARGUMENTS];
    let mut environment_pointers = [None; MAX_USER_ENTRY_ENVIRONMENT];

    for index in (0..arguments.len()).rev() {
        argument_pointers[index] = Some(push_c_string_bytes(&mut stack_cursor, arguments[index]));
    }
    for index in (0..environment.len()).rev() {
        environment_pointers[index] =
            Some(push_c_string_bytes(&mut stack_cursor, environment[index]));
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
/// Panics if the address is outside the user program/mapping region, a
/// physical frame cannot be allocated, or page-table mapping fails.
pub fn allocate_and_map_user_page(
    address_space: UserAddressSpace,
    frame_allocator: &mut PhysicalFrameAllocator,
    virtual_page_start: UserPageStart,
    flags: PageTableFlags,
    owner: FrameRangeOwner,
) -> PhysicalFrameStart {
    let virtual_address = virtual_page_start.as_u64();
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
        address_space.map_user_page(frame_allocator, virtual_page_start, physical_address, flags);
    }

    physical_address
}

/// Return whether the user stack is writable and its guard page is unmapped.
///
/// # Panics
///
/// Panics if the stack has no pages or the stack size overflows.
pub fn verify_user_stack_mapping(
    address_space: UserAddressSpace,
    stack: AllocatedUserStack,
) -> bool {
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

    address_space.is_user_range_mapped_writable(stack.base().as_usize(), stack_size)
        && !address_space.is_user_range_mapped_readable(
            usize::try_from(guard_page).expect("user stack guard must fit in usize"),
            PAGE_SIZE_USIZE,
        )
}

struct UserStackCursor {
    stack: AllocatedUserStack,
    pointer: UserVirtualAddress,
}

impl UserStackCursor {
    fn new(stack: AllocatedUserStack, pointer: UserVirtualAddress) -> Self {
        assert!(
            pointer.as_u64() > stack.base().as_u64(),
            "user stack cursor must start above the stack base"
        );
        Self { stack, pointer }
    }

    fn push_bytes(&mut self, byte_count: u64) -> UserVirtualAddress {
        let next_pointer = self
            .pointer
            .checked_sub(byte_count)
            .expect("user stack cursor underflowed");
        assert!(
            next_pointer.as_u64() >= self.stack.base().as_u64(),
            "user entry data must fit in the mapped user stack"
        );
        self.pointer = next_pointer;
        self.pointer
    }

    fn align_down(&mut self, alignment: u64) {
        debug_assert!(alignment.is_power_of_two());
        let aligned_pointer = self.pointer.as_u64() & !(alignment - 1);
        let aligned_pointer = UserVirtualAddress::new(VirtAddr::new(aligned_pointer))
            .expect("aligned user stack pointer must remain valid");
        assert!(
            aligned_pointer.as_u64() >= self.stack.base().as_u64(),
            "aligned user stack pointer must stay inside the mapped user stack"
        );
        self.pointer = aligned_pointer;
    }

    fn into_user_address(self) -> UserVirtualAddress {
        self.pointer
    }
}

fn push_c_string_bytes(stack_cursor: &mut UserStackCursor, value: &[u8]) -> UserVirtualAddress {
    let length = u64::try_from(value.len().saturating_add(1))
        .expect("user entry string length must fit in u64");
    let stack_pointer = stack_cursor.push_bytes(length);

    let destination = stack_cursor.stack_pointer(stack_pointer);
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
        stack_cursor.write_u64(slot_address, pointer);
    }
    let terminator_offset = u64::try_from(pointers.len())
        .expect("user entry pointer terminator index must fit in u64")
        .checked_mul(POINTER_SIZE)
        .expect("user entry pointer terminator offset overflowed");
    let terminator_address = stack_pointer
        .checked_add(terminator_offset)
        .expect("user entry pointer terminator address overflowed");
    stack_cursor.write_u64(terminator_address, 0);

    stack_pointer
}

impl UserStackCursor {
    fn stack_pointer(&self, address: UserVirtualAddress) -> *mut u8 {
        let offset = address
            .as_u64()
            .checked_sub(self.stack.base().as_u64())
            .expect("user stack write address must be inside stack");
        assert!(
            offset < self.stack.byte_len(),
            "user stack write address must stay inside stack"
        );
        let physical_address = self
            .stack
            .physical_range()
            .start()
            .as_address()
            .checked_add(offset)
            .expect("user stack physical write address overflowed");
        physical_address.as_usize() as *mut u8
    }

    fn write_u64(&self, address: UserVirtualAddress, value: u64) {
        let bytes = value.to_ne_bytes();
        let pointer = self.stack_pointer(address);
        // SAFETY: The destination was translated into the physical frame range
        // backing the prepared stack and reserved for one 64-bit pointer value.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
        }
    }
}
