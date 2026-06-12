/// Kernel Heap Allocator.
/// Uses `linked_list_allocator` for managing the kernel heap.
/// Register as #[`global_allocator`] to enable Box/Vec/String, etc.
use crate::kernel::memory::address::PhysicalFrameStart;

use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap size: 32MB
pub const HEAP_SIZE: usize = 32 * 1024 * 1024;
/// Number of pages required for the heap
pub const HEAP_PAGES: u64 = (HEAP_SIZE / 4096) as u64;

/// Initialize the heap at a 4 KiB-aligned physical frame start.
///
/// # Safety
/// The memory from `heap_start` to `heap_start` + `HEAP_SIZE` must be valid,
/// unused, and identity-mapped into the active kernel address space.
pub unsafe fn init(heap_start: PhysicalFrameStart) {
    ALLOCATOR
        .lock()
        .init(heap_start.as_usize() as *mut u8, HEAP_SIZE);
}
