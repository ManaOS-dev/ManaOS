//! Kernel task stack owner metadata.

use crate::kernel::memory::address::{KernelVirtualRange, PhysicalFrameRange, VirtAddr};
use crate::kernel::memory::frame_allocator::{FrameRangeOwner, PhysicalFrameAllocator};
use crate::kernel::memory::paging;
use crate::kernel::memory::virtual_allocator::KernelVirtualRangeAllocator;

const PAGE_SIZE: usize = 4096;
const PAGE_SIZE_U64: u64 = 4096;
const DEFAULT_KERNEL_STACK_SIZE: usize = 16 * 1024;
const DEFAULT_KERNEL_STACK_WRITABLE_PAGES: u64 = 4;
const KERNEL_STACK_GUARD_PAGES: u64 = 1;

/// The schedulable task kind that owns a guarded kernel stack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KernelStackFaultOwner {
    /// A kernel task owns the guarded stack.
    KernelTask,
    /// A user task owns the guarded kernel stack.
    UserTask,
}

impl KernelStackFaultOwner {
    /// Return a stable diagnostic label for this stack owner.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KernelTask => "kernel_task",
            Self::UserTask => "user_task",
        }
    }
}

/// Diagnostic metadata for a fault inside a known kernel stack guard page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelStackGuardFault {
    task_identifier: u64,
    owner: KernelStackFaultOwner,
    guard_page_start: u64,
    writable_start: u64,
    stack_top: u64,
}

impl KernelStackGuardFault {
    /// Create a kernel stack guard-fault diagnostic record.
    pub(super) const fn new(
        task_identifier: u64,
        owner: KernelStackFaultOwner,
        guard_page_start: u64,
        writable_start: u64,
        stack_top: u64,
    ) -> Self {
        Self {
            task_identifier,
            owner,
            guard_page_start,
            writable_start,
            stack_top,
        }
    }

    /// Return the task identifier that owns the guard page.
    pub const fn task_identifier(self) -> u64 {
        self.task_identifier
    }

    /// Return the owner kind for the guarded stack.
    pub const fn owner(self) -> KernelStackFaultOwner {
        self.owner
    }

    /// Return the start address of the unmapped guard page.
    pub const fn guard_page_start(self) -> u64 {
        self.guard_page_start
    }

    /// Return the first mapped writable stack address.
    pub const fn writable_start(self) -> u64 {
        self.writable_start
    }

    /// Return the guarded stack top address.
    pub const fn stack_top(self) -> u64 {
        self.stack_top
    }
}

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
        frame_allocator: &mut PhysicalFrameAllocator,
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

    /// Return whether `address` is inside this stack's unmapped guard page.
    pub(super) fn contains_guard_address(&self, address: u64) -> bool {
        let guard_start = self.guard_page_virtual_start();
        let guard_end = guard_start
            .checked_add(PAGE_SIZE_U64)
            .expect("kernel stack guard range end overflowed");
        address >= guard_start && address < guard_end
    }

    /// Return whether a byte range is fully inside this stack's writable pages.
    pub(super) fn contains_writable_range(&self, start_address: u64, byte_len: u64) -> bool {
        if byte_len == 0 {
            return false;
        }

        let Some(end_address) = start_address.checked_add(byte_len) else {
            return false;
        };
        start_address >= self.writable_virtual_start() && end_address <= self.virtual_top()
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
