/// Kernel Heap Allocator.
/// Uses `linked_list_allocator` for managing the kernel heap.
/// Register as #[`global_allocator`] to enable Box/Vec/String, etc.
use crate::kernel::memory::address::PhysicalFrameRange;

use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap size: 32MB
pub const HEAP_SIZE: usize = 32 * 1024 * 1024;
/// Number of pages required for the heap
pub const HEAP_PAGES: u64 = (HEAP_SIZE / 4096) as u64;

/// Initialize the heap inside a contiguous physical frame range.
///
/// # Safety
/// The memory from `heap_range.start()` through `HEAP_SIZE` bytes must be
/// valid, unused, and identity-mapped into the active kernel address space.
///
/// # Panics
///
/// Panics if `heap_range` is smaller than [`HEAP_SIZE`].
pub unsafe fn init(heap_range: PhysicalFrameRange) {
    assert!(
        heap_range.page_count() >= HEAP_PAGES,
        "heap frame range must cover the configured heap pages"
    );
    let heap_bytes = heap_range.byte_len();
    assert!(
        heap_bytes >= HEAP_SIZE as u64,
        "heap frame range byte length must cover the configured heap size"
    );
    ALLOCATOR
        .lock()
        .init(heap_range.start().as_usize() as *mut u8, HEAP_SIZE);
}
