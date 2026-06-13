//! # `kernel::memory::virtual_allocator`
//!
//! ## Owns
//! - Kernel virtual address range reservation for future dynamic mappings
//! - Monotonic allocation of non-overlapping kernel virtual page ranges
//!
//! ## Does NOT own
//! - Page-table mapping or unmapping (-> `kernel::memory::paging`)
//! - Physical frame allocation (-> `kernel::memory::frame_allocator`)
//! - Task stack metadata (-> `kernel::task::stack`)
//!
//! ## Public API
//! - [`KernelVirtualRangeAllocator`] - Monotonic kernel virtual range allocator
//! - [`new_dynamic_mapping_allocator`] - Create the default dynamic mapping allocator
//! - [`verify_kernel_virtual_range_allocation`] - Boot-time allocation self-check
//! - [`verify_kernel_virtual_range_exhaustion`] - Boot-time exhaustion self-check

use super::address::{KernelVirtualRange, VirtAddr};

const PAGE_SIZE: u64 = 4096;
const DYNAMIC_MAPPING_START: u64 = 0xffff_8000_0000_0000;
const DYNAMIC_MAPPING_PAGE_COUNT: u64 = 262_144;

/// A monotonic allocator for reserved kernel virtual page ranges.
pub struct KernelVirtualRangeAllocator {
    next_start: VirtAddr,
    remaining_pages: u64,
}

impl KernelVirtualRangeAllocator {
    /// Create a kernel virtual range allocator from a page-aligned start and
    /// total page count.
    pub const fn new(start: VirtAddr, page_count: u64) -> Option<Self> {
        let Some(_) = KernelVirtualRange::new(start, page_count) else {
            return None;
        };

        Some(Self {
            next_start: start,
            remaining_pages: page_count,
        })
    }

    /// Allocate a non-overlapping kernel virtual range with `page_count` pages.
    pub fn allocate_pages(&mut self, page_count: u64) -> Option<KernelVirtualRange> {
        if page_count == 0 || page_count > self.remaining_pages {
            return None;
        }

        let byte_len = page_count.checked_mul(PAGE_SIZE)?;
        let range = KernelVirtualRange::new(self.next_start, page_count)?;
        let next_start = self.next_start.checked_add(byte_len)?;
        self.next_start = next_start;
        self.remaining_pages -= page_count;
        Some(range)
    }

    /// Return the number of unallocated pages left in the allocator.
    pub const fn remaining_pages(&self) -> u64 {
        self.remaining_pages
    }
}

/// Create the default kernel dynamic mapping range allocator.
///
/// # Panics
///
/// Panics if the compiled-in dynamic mapping range is invalid.
pub fn new_dynamic_mapping_allocator() -> KernelVirtualRangeAllocator {
    KernelVirtualRangeAllocator::new(
        VirtAddr::new(DYNAMIC_MAPPING_START),
        DYNAMIC_MAPPING_PAGE_COUNT,
    )
    .expect("kernel dynamic mapping virtual range must be valid")
}

/// Verify that kernel virtual range allocation is monotonic and non-overlapping.
pub fn verify_kernel_virtual_range_allocation() -> bool {
    let mut allocator = new_dynamic_mapping_allocator();
    let Some(first_range) = allocator.allocate_pages(1) else {
        return false;
    };
    let Some(second_range) = allocator.allocate_pages(2) else {
        return false;
    };

    first_range.start().as_u64() == DYNAMIC_MAPPING_START
        && first_range.page_count() == 1
        && first_range.byte_len() == PAGE_SIZE
        && first_range.end_exclusive() == second_range.start()
        && second_range.page_count() == 2
        && second_range.byte_len() == PAGE_SIZE * 2
        && allocator.remaining_pages() == DYNAMIC_MAPPING_PAGE_COUNT - 3
}

/// Verify that invalid and exhausted kernel virtual allocations are rejected.
pub fn verify_kernel_virtual_range_exhaustion() -> bool {
    let Some(mut allocator) =
        KernelVirtualRangeAllocator::new(VirtAddr::new(DYNAMIC_MAPPING_START), 2)
    else {
        return false;
    };

    let zero_page_rejected = allocator.allocate_pages(0).is_none();
    let first_page_allocated = allocator.allocate_pages(1).is_some();
    let second_page_allocated = allocator.allocate_pages(1).is_some();
    let exhausted_rejected = allocator.allocate_pages(1).is_none();
    let misaligned_rejected =
        KernelVirtualRangeAllocator::new(VirtAddr::new(DYNAMIC_MAPPING_START + 1), 1).is_none();

    zero_page_rejected
        && first_page_allocated
        && second_page_allocated
        && exhausted_rejected
        && misaligned_rejected
        && allocator.remaining_pages() == 0
}
