//! Kernel task stack owner metadata.

use crate::kernel::memory::address::{KernelVirtualRange, VirtAddr};
use crate::kernel::memory::virtual_allocator::KernelVirtualRangeAllocator;
use alloc::boxed::Box;
use alloc::vec;

const PAGE_SIZE: usize = 4096;
const DEFAULT_KERNEL_STACK_SIZE: usize = 16 * 1024;
const DEFAULT_KERNEL_STACK_WRITABLE_PAGES: u64 = 4;
const KERNEL_STACK_GUARD_PAGES: u64 = 1;

struct KernelStackVirtualReservation {
    range: KernelVirtualRange,
    writable_page_count: u64,
}

impl KernelStackVirtualReservation {
    fn new(allocator: &mut KernelVirtualRangeAllocator, writable_page_count: u64) -> Option<Self> {
        let reserved_page_count = writable_page_count.checked_add(KERNEL_STACK_GUARD_PAGES)?;
        let range = allocator.allocate_pages(reserved_page_count)?;
        Some(Self {
            range,
            writable_page_count,
        })
    }

    fn guard_page_start(&self) -> VirtAddr {
        self.range.start()
    }

    fn writable_start(&self) -> VirtAddr {
        self.range
            .start()
            .checked_add(PAGE_SIZE as u64)
            .expect("kernel stack writable range must follow the guard page")
    }

    fn stack_top(&self) -> VirtAddr {
        self.range.end_exclusive()
    }

    fn reserved_page_count(&self) -> u64 {
        self.range.page_count()
    }

    fn writable_page_count(&self) -> u64 {
        self.writable_page_count
    }
}

/// Heap-backed kernel stack owned by one schedulable task.
///
/// This is the transitional metadata shape before guarded kernel stack
/// mappings exist. The buffer keeps the stack memory alive for the lifetime of
/// the task context, and the explicit top/base accessors define the future
/// replacement boundary for guarded mapped stacks.
pub(super) struct KernelStack {
    buffer: Box<[u8]>,
    virtual_reservation: KernelStackVirtualReservation,
}

impl KernelStack {
    /// Allocate the current default heap-backed kernel stack.
    pub(super) fn new_default(allocator: &mut KernelVirtualRangeAllocator) -> Self {
        debug_assert_eq!(
            DEFAULT_KERNEL_STACK_SIZE,
            PAGE_SIZE
                * usize::try_from(DEFAULT_KERNEL_STACK_WRITABLE_PAGES)
                    .expect("kernel stack page count must fit in usize")
        );
        let virtual_reservation =
            KernelStackVirtualReservation::new(allocator, DEFAULT_KERNEL_STACK_WRITABLE_PAGES)
                .expect("kernel stack virtual reservation allocator must have capacity");
        Self {
            buffer: vec![0; DEFAULT_KERNEL_STACK_SIZE].into_boxed_slice(),
            virtual_reservation,
        }
    }

    /// Return the lowest writable address in this stack buffer.
    pub(super) fn base(&self) -> usize {
        self.buffer.as_ptr() as usize
    }

    /// Return one byte past the highest writable address in this stack buffer.
    pub(super) fn top(&self) -> usize {
        self.buffer.as_ptr() as usize + self.buffer.len()
    }

    /// Return the writable stack size in bytes.
    pub(super) fn byte_len(&self) -> usize {
        self.buffer.len()
    }

    /// Return the reserved guard page virtual start address.
    pub(super) fn guard_page_virtual_start(&self) -> u64 {
        self.virtual_reservation.guard_page_start().as_u64()
    }

    /// Return the first virtual address reserved for future writable stack mapping.
    pub(super) fn writable_virtual_start(&self) -> u64 {
        self.virtual_reservation.writable_start().as_u64()
    }

    /// Return the future guarded stack virtual top address.
    pub(super) fn virtual_top(&self) -> u64 {
        self.virtual_reservation.stack_top().as_u64()
    }

    /// Return the number of reserved virtual pages including the guard page.
    pub(super) fn reserved_page_count(&self) -> u64 {
        self.virtual_reservation.reserved_page_count()
    }

    /// Return the number of future writable stack pages.
    pub(super) fn writable_page_count(&self) -> u64 {
        self.virtual_reservation.writable_page_count()
    }
}
