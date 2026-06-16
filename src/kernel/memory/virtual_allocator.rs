//! # `kernel::memory::virtual_allocator`
//!
//! ## Owns
//! - Kernel virtual address range reservation for future dynamic mappings
//! - Reusable allocation of non-overlapping kernel virtual page ranges
//!
//! ## Does NOT own
//! - Page-table mapping or unmapping (-> `kernel::memory::paging`)
//! - Physical frame allocation (-> `kernel::memory::frame_allocator`)
//! - Task stack metadata (-> `kernel::task::stack`)
//!
//! ## Public API
//! - [`KernelVirtualRangeAllocator`] - Reusable kernel virtual range allocator
//! - [`new_dynamic_mapping_allocator`] - Create the default dynamic mapping allocator
//! - [`verify_kernel_virtual_range_allocation`] - Boot-time allocation self-check
//! - [`verify_kernel_virtual_range_exhaustion`] - Boot-time exhaustion self-check

use super::address::{KernelVirtualRange, PageCount, VirtAddr};

const PAGE_SIZE: u64 = 4096;
const DYNAMIC_MAPPING_START: u64 = 0xffff_8000_0000_0000;
const DYNAMIC_MAPPING_PAGE_COUNT: u64 = 262_144;
const MAX_FREE_RANGES: usize = 128;

#[derive(Clone, Copy)]
struct VirtualFreeRange {
    start: VirtAddr,
    page_count: u64,
}

impl VirtualFreeRange {
    const fn empty() -> Self {
        Self {
            start: VirtAddr::new(0),
            page_count: 0,
        }
    }

    fn end_exclusive(self) -> Option<VirtAddr> {
        let byte_len = self.page_count.checked_mul(PAGE_SIZE)?;
        self.start.checked_add(byte_len)
    }
}

/// A reusable allocator for reserved kernel virtual page ranges.
pub struct KernelVirtualRangeAllocator {
    managed_start: VirtAddr,
    managed_page_count: PageCount,
    free_ranges: [VirtualFreeRange; MAX_FREE_RANGES],
    free_count: usize,
}

impl KernelVirtualRangeAllocator {
    /// Create a kernel virtual range allocator from a page-aligned start and
    /// total page count.
    pub const fn new(start: VirtAddr, page_count: PageCount) -> Option<Self> {
        let Some(_) = KernelVirtualRange::new(start, page_count) else {
            return None;
        };

        Some(Self {
            managed_start: start,
            managed_page_count: page_count,
            free_ranges: {
                let mut ranges = [VirtualFreeRange::empty(); MAX_FREE_RANGES];
                ranges[0] = VirtualFreeRange {
                    start,
                    page_count: page_count.as_u64(),
                };
                ranges
            },
            free_count: 1,
        })
    }

    /// Allocate a non-overlapping kernel virtual range with `page_count` pages.
    pub fn allocate_pages(&mut self, page_count: PageCount) -> Option<KernelVirtualRange> {
        let pages = page_count.as_u64();
        for index in 0..self.free_count {
            let free_range = self.free_ranges[index];
            if free_range.page_count < pages {
                continue;
            }

            let range = KernelVirtualRange::new(free_range.start, page_count)?;
            if free_range.page_count == pages {
                self.remove_free_range_at(index);
            } else {
                let byte_len = page_count.byte_len();
                self.free_ranges[index].start = free_range.start.checked_add(byte_len)?;
                self.free_ranges[index].page_count -= pages;
            }
            return Some(range);
        }

        None
    }

    /// Release a previously allocated kernel virtual range.
    pub fn free_pages(&mut self, range: KernelVirtualRange) -> bool {
        if !self.contains_range(range) {
            return false;
        }

        let mut insert_index = 0;
        while insert_index < self.free_count
            && self.free_ranges[insert_index].start.as_u64() < range.start().as_u64()
        {
            insert_index += 1;
        }

        if insert_index > 0 {
            let previous = self.free_ranges[insert_index - 1];
            let Some(previous_end) = previous.end_exclusive() else {
                return false;
            };
            if previous_end.as_u64() > range.start().as_u64() {
                return false;
            }
        }
        if insert_index < self.free_count {
            let next = self.free_ranges[insert_index];
            if range.end_exclusive().as_u64() > next.start.as_u64() {
                return false;
            }
        }

        if self.free_count >= MAX_FREE_RANGES {
            return false;
        }

        for move_index in (insert_index..self.free_count).rev() {
            self.free_ranges[move_index + 1] = self.free_ranges[move_index];
        }
        self.free_ranges[insert_index] = VirtualFreeRange {
            start: range.start(),
            page_count: range.page_count(),
        };
        self.free_count += 1;
        self.merge_adjacent_free_ranges();
        true
    }

    /// Return the number of unallocated pages left in the allocator.
    pub fn remaining_pages(&self) -> u64 {
        let mut pages = 0_u64;
        for index in 0..self.free_count {
            pages = pages.saturating_add(self.free_ranges[index].page_count);
        }
        pages
    }

    fn contains_range(&self, range: KernelVirtualRange) -> bool {
        let managed_byte_len = self.managed_page_count.byte_len();
        let Some(managed_end) = self.managed_start.checked_add(managed_byte_len) else {
            return false;
        };

        range.start().as_u64() >= self.managed_start.as_u64()
            && range.end_exclusive().as_u64() <= managed_end.as_u64()
    }

    fn remove_free_range_at(&mut self, index: usize) {
        for move_index in index..self.free_count - 1 {
            self.free_ranges[move_index] = self.free_ranges[move_index + 1];
        }
        self.free_count -= 1;
    }

    fn merge_adjacent_free_ranges(&mut self) {
        if self.free_count < 2 {
            return;
        }

        let mut write_index = 0;
        for read_index in 1..self.free_count {
            let current = self.free_ranges[write_index];
            let next = self.free_ranges[read_index];
            if current.end_exclusive() == Some(next.start) {
                self.free_ranges[write_index].page_count = self.free_ranges[write_index]
                    .page_count
                    .saturating_add(next.page_count);
            } else {
                write_index += 1;
                self.free_ranges[write_index] = next;
            }
        }
        self.free_count = write_index + 1;
    }
}

/// Create the default kernel dynamic mapping range allocator.
///
/// # Panics
///
/// Panics if the compiled-in dynamic mapping range is invalid.
pub fn new_dynamic_mapping_allocator() -> KernelVirtualRangeAllocator {
    let page_count = PageCount::new(DYNAMIC_MAPPING_PAGE_COUNT)
        .expect("dynamic mapping page count must be valid");
    KernelVirtualRangeAllocator::new(VirtAddr::new(DYNAMIC_MAPPING_START), page_count)
        .expect("kernel dynamic mapping virtual range must be valid")
}

/// Verify that kernel virtual range allocation is non-overlapping and reusable.
pub fn verify_kernel_virtual_range_allocation() -> bool {
    let mut allocator = new_dynamic_mapping_allocator();
    let Some(first_range) = allocator.allocate_pages(page_count(1)) else {
        return false;
    };
    let Some(second_range) = allocator.allocate_pages(page_count(2)) else {
        return false;
    };
    if !allocator.free_pages(first_range) {
        return false;
    }
    let Some(reused_range) = allocator.allocate_pages(page_count(1)) else {
        return false;
    };

    first_range.start().as_u64() == DYNAMIC_MAPPING_START
        && first_range.page_count() == 1
        && first_range.byte_len() == PAGE_SIZE
        && first_range.end_exclusive() == second_range.start()
        && second_range.page_count() == 2
        && second_range.byte_len() == PAGE_SIZE * 2
        && reused_range == first_range
        && allocator.remaining_pages() == DYNAMIC_MAPPING_PAGE_COUNT - 3
}

/// Verify that invalid and exhausted kernel virtual allocations are rejected.
pub fn verify_kernel_virtual_range_exhaustion() -> bool {
    let Some(mut allocator) =
        KernelVirtualRangeAllocator::new(VirtAddr::new(DYNAMIC_MAPPING_START), page_count(2))
    else {
        return false;
    };

    let zero_page_rejected = PageCount::new(0).is_none();
    let first_page_allocated = allocator.allocate_pages(page_count(1)).is_some();
    let second_page_allocated = allocator.allocate_pages(page_count(1)).is_some();
    let exhausted_rejected = allocator.allocate_pages(page_count(1)).is_none();
    let misaligned_rejected =
        KernelVirtualRangeAllocator::new(VirtAddr::new(DYNAMIC_MAPPING_START + 1), page_count(1))
            .is_none();
    let outside_range_rejected = match KernelVirtualRange::new(
        VirtAddr::new(DYNAMIC_MAPPING_START - PAGE_SIZE),
        page_count(1),
    ) {
        Some(range) => !allocator.free_pages(range),
        None => true,
    };

    zero_page_rejected
        && first_page_allocated
        && second_page_allocated
        && exhausted_rejected
        && misaligned_rejected
        && outside_range_rejected
        && allocator.remaining_pages() == 0
}

const fn page_count(count: u64) -> PageCount {
    PageCount::new(count).expect("virtual allocator page count must be valid")
}
