//! Kernel task stack owner metadata.

use crate::kernel::memory::address::{KernelVirtualRange, PhysicalFrameRange, VirtAddr};
use crate::kernel::memory::frame_allocator::{BumpFrameAllocator, FrameRangeOwner};
use crate::kernel::memory::paging;
use crate::kernel::memory::virtual_allocator::KernelVirtualRangeAllocator;

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

    fn guard_range(&self) -> KernelVirtualRange {
        KernelVirtualRange::new(self.guard_page_start(), KERNEL_STACK_GUARD_PAGES)
            .expect("kernel stack guard range must be valid")
    }

    fn writable_start(&self) -> VirtAddr {
        self.range
            .start()
            .checked_add(PAGE_SIZE as u64)
            .expect("kernel stack writable range must follow the guard page")
    }

    fn writable_range(&self) -> KernelVirtualRange {
        KernelVirtualRange::new(self.writable_start(), self.writable_page_count)
            .expect("kernel stack writable range must be valid")
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

/// Guarded mapped kernel stack owned by one schedulable task.
///
/// The lowest reserved virtual page is intentionally unmapped as the guard
/// page. Writable pages are mapped kernel-only and non-executable.
pub(super) struct KernelStack {
    virtual_reservation: KernelStackVirtualReservation,
    physical_range: PhysicalFrameRange,
}

impl KernelStack {
    /// Allocate the current default guarded mapped kernel stack.
    ///
    /// # Panics
    ///
    /// Panics if physical stack frames cannot be allocated, virtual range
    /// reservation is exhausted, or page-table mapping fails.
    pub(super) fn new_default(
        frame_allocator: &mut BumpFrameAllocator,
        virtual_range_allocator: &mut KernelVirtualRangeAllocator,
    ) -> Self {
        debug_assert_eq!(
            DEFAULT_KERNEL_STACK_SIZE,
            PAGE_SIZE
                * usize::try_from(DEFAULT_KERNEL_STACK_WRITABLE_PAGES)
                    .expect("kernel stack page count must fit in usize")
        );
        let virtual_reservation = KernelStackVirtualReservation::new(
            virtual_range_allocator,
            DEFAULT_KERNEL_STACK_WRITABLE_PAGES,
        )
        .expect("kernel stack virtual reservation allocator must have capacity");
        let physical_range = frame_allocator
            .allocate_frames_for(
                DEFAULT_KERNEL_STACK_WRITABLE_PAGES,
                FrameRangeOwner::KernelStack,
            )
            .expect("OOM: failed to allocate kernel stack frames");
        let physical_stack_pointer = physical_range.start().as_usize() as *mut u8;

        // SAFETY: `physical_range` is freshly allocated, identity mapped, and
        // exclusively owned by this kernel stack.
        unsafe {
            core::ptr::write_bytes(physical_stack_pointer, 0, DEFAULT_KERNEL_STACK_SIZE);
        }

        let mapped_start = paging::map_kernel_writable_no_execute_range(
            frame_allocator,
            virtual_reservation.writable_range(),
            physical_range,
        );
        assert_eq!(
            mapped_start.as_u64(),
            virtual_reservation.writable_start().as_u64(),
            "kernel stack writable mapping must start after the guard page"
        );
        assert!(
            paging::is_kernel_range_unmapped(virtual_reservation.guard_range()),
            "kernel stack guard page must remain unmapped"
        );
        assert!(
            paging::is_kernel_range_mapped_writable_no_execute(
                virtual_reservation.writable_range()
            ),
            "kernel stack writable pages must be kernel-only writable NX"
        );

        Self {
            virtual_reservation,
            physical_range,
        }
    }

    /// Return the lowest mapped writable virtual address in this stack.
    pub(super) fn base(&self) -> usize {
        usize::try_from(self.writable_virtual_start()).expect("kernel stack base must fit in usize")
    }

    /// Return one byte past the highest mapped writable address in this stack.
    pub(super) fn top(&self) -> usize {
        usize::try_from(self.virtual_top()).expect("kernel stack top must fit in usize")
    }

    /// Return the writable stack size in bytes.
    pub(super) fn byte_len(&self) -> usize {
        usize::try_from(self.physical_range.byte_len())
            .expect("kernel stack byte length must fit in usize")
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
