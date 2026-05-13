/// Kernel Heap Allocator.
/// Uses linked_list_allocator for managing the kernel heap.
/// Register as #[global_allocator] to enable Box/Vec/String, etc.
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap size: 32MB
pub const HEAP_SIZE: usize = 32 * 1024 * 1024;
/// Number of pages required for the heap
pub const HEAP_PAGES: u64 = (HEAP_SIZE / 4096) as u64;

/// Initialize the heap. heap_start must be a 4KB aligned physical address.
///
/// # Safety
/// The memory from heap_start to heap_start + HEAP_SIZE must be valid and unused.
pub unsafe fn init(heap_start: usize) {
    ALLOCATOR.lock().init(heap_start as *mut u8, HEAP_SIZE);
}
