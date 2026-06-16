//! A reusable physical frame allocator with owner tracking.

use super::address::{FrameCount, PhysAddr, PhysicalFrameRange, PhysicalFrameStart};

mod verification;

pub use verification::{
    verify_contiguous_allocation_boundaries, verify_duplicate_allocation_rejection,
    verify_explicit_owner_coverage, verify_owner_tracking, verify_released_frame_reuse,
    verify_reserved_range_exclusion, verify_reserved_used_and_free_range_tracking,
    verify_typed_physical_frame_start, verify_zero_address_skip_for_multi_frame_allocations,
};

const MAX_REGIONS: usize = 128;
const MAX_TRACKED_RANGES: usize = 512;
const FRAME_SIZE: u64 = 4096;

#[derive(Clone, Copy)]
struct Region {
    start: u64,
    pages: u64,
}

#[derive(Clone, Copy)]
struct TrackedRange {
    region: Region,
    state: FrameRangeState,
    owner: FrameRangeOwner,
}

/// Physical frame range state tracked by the frame allocator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameRangeState {
    /// Frames are unavailable for allocation.
    Reserved,
    /// Frames are available for allocation.
    Free,
    /// Frames are owned by a kernel subsystem, user mapping, page table, or device.
    Used,
}

/// Owner recorded for a tracked physical frame range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameRangeOwner {
    /// Frames are available and have no active owner.
    Free,
    /// Frames are unavailable because firmware did not report them as conventional memory.
    FirmwareReserved,
    /// Frames contain the loaded kernel image and must never be reused.
    KernelImage,
    /// Frames are reserved for memory-mapped I/O and must never be allocated as RAM.
    Mmio,
    /// Frames are reserved as unmapped guard pages.
    GuardPage,
    /// Frames are used by an owner that has not been classified yet.
    UnknownUsed,
    /// Frames store page-table structures.
    PageTable,
    /// Frames back the kernel heap.
    KernelHeap,
    /// Frames back guarded kernel stacks.
    KernelStack,
    /// Frames back the display backbuffer.
    FramebufferBackbuffer,
    /// Frames back Advanced Host Controller Interface DMA buffers.
    AhciDma,
    /// Frames back temporary dynamic kernel mappings.
    DynamicKernelMapping,
    /// Frames back a user stack.
    UserStack,
    /// Frames back loaded user ELF segments.
    UserElf,
    /// Frames back user heap growth.
    UserHeap,
    /// Frames back private user memory mappings.
    UserMapping,
}

/// Page totals for tracked physical frame ranges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameAllocatorStatistics {
    /// Number of tracked reserved 4 KiB frames.
    pub reserved: u64,
    /// Number of tracked free 4 KiB frames.
    pub free: u64,
    /// Number of tracked used 4 KiB frames.
    pub used: u64,
}

/// Page totals grouped by tracked frame-range owner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameAllocatorOwnerStatistics {
    /// Number of frames with no active owner.
    pub free: u64,
    /// Number of firmware-reserved frames.
    pub firmware_reserved: u64,
    /// Number of frames containing the loaded kernel image.
    pub kernel_image: u64,
    /// Number of memory-mapped I/O frames.
    pub mmio: u64,
    /// Number of guard-page frames.
    pub guard_page: u64,
    /// Number of used frames whose owner has not been classified.
    pub unknown_used: u64,
    /// Number of page-table frames.
    pub page_table: u64,
    /// Number of kernel heap frames.
    pub kernel_heap: u64,
    /// Number of guarded kernel stack frames.
    pub kernel_stack: u64,
    /// Number of framebuffer backbuffer frames.
    pub framebuffer_backbuffer: u64,
    /// Number of AHCI DMA frames.
    pub ahci_dma: u64,
    /// Number of temporary dynamic kernel mapping frames.
    pub dynamic_kernel_mapping: u64,
    /// Number of user stack frames.
    pub user_stack: u64,
    /// Number of user ELF frames.
    pub user_elf: u64,
    /// Number of user heap frames.
    pub user_heap: u64,
    /// Number of private user mapping frames.
    pub user_mapping: u64,
}

/// A reusable allocator for 4 KiB physical frames registered from UEFI memory map
/// regions.
pub struct PhysicalFrameAllocator {
    regions: [Region; MAX_REGIONS],
    count: usize,
    tracked_ranges: [TrackedRange; MAX_TRACKED_RANGES],
    tracked_count: usize,
}

impl PhysicalFrameAllocator {
    /// Create an empty physical frame allocator.
    pub const fn new() -> Self {
        Self {
            regions: [Region { start: 0, pages: 0 }; MAX_REGIONS],
            count: 0,
            tracked_ranges: [TrackedRange {
                region: Region { start: 0, pages: 0 },
                state: FrameRangeState::Reserved,
                owner: FrameRangeOwner::FirmwareReserved,
            }; MAX_TRACKED_RANGES],
            tracked_count: 0,
        }
    }

    /// Register a conventional memory region (call before `ExitBootServices`).
    pub fn add_region(&mut self, start: PhysAddr, frame_count: FrameCount) {
        let Some(region) = normalize_region(start, frame_count) else {
            return;
        };

        self.insert_region(region);
        self.insert_tracked_range(region, FrameRangeState::Free, FrameRangeOwner::Free);
    }

    /// Register a reserved physical memory region.
    pub fn reserve_region(&mut self, start: PhysAddr, frame_count: FrameCount) {
        self.reserve_region_for(start, frame_count, FrameRangeOwner::FirmwareReserved);
    }

    /// Register a reserved physical memory region with an explicit owner.
    pub fn reserve_region_for(
        &mut self,
        start: PhysAddr,
        frame_count: FrameCount,
        owner: FrameRangeOwner,
    ) {
        let Some(region) = normalize_reserved_region(start, frame_count) else {
            return;
        };

        self.insert_tracked_range(region, FrameRangeState::Reserved, owner);
        self.remove_reserved_region_from_free_ranges(region);
    }

    /// Allocate a single 4KiB frame.
    #[allow(dead_code)]
    pub fn allocate_frame(&mut self) -> Option<PhysicalFrameStart> {
        self.allocate_frame_for(FrameRangeOwner::UnknownUsed)
    }

    /// Allocate a single 4KiB frame for `owner`.
    pub fn allocate_frame_for(&mut self, owner: FrameRangeOwner) -> Option<PhysicalFrameStart> {
        self.allocate_frames_for(
            FrameCount::new(1).expect("single-frame count must be valid"),
            owner,
        )
        .map(PhysicalFrameRange::start)
    }

    /// Allocate a contiguous 4KiB frame range.
    /// Contiguous allocation is only guaranteed within a single region.
    pub fn allocate_frames(&mut self, frame_count: FrameCount) -> Option<PhysicalFrameRange> {
        self.allocate_frames_for(frame_count, FrameRangeOwner::UnknownUsed)
    }

    /// Allocate a contiguous 4KiB frame range for `owner`.
    ///
    /// Contiguous allocation is only guaranteed within a single region.
    pub fn allocate_frames_for(
        &mut self,
        frame_count: FrameCount,
        owner: FrameRangeOwner,
    ) -> Option<PhysicalFrameRange> {
        assert_ne!(
            owner,
            FrameRangeOwner::Free,
            "allocated frames must record a non-free owner"
        );

        let frames = frame_count.as_u64();
        for index in 0..self.tracked_count {
            let tracked = self.tracked_ranges[index];
            if tracked.state != FrameRangeState::Free || tracked.region.pages < frames {
                continue;
            }

            let candidate_region = Region {
                start: tracked.region.start,
                pages: frames,
            };
            if self.mark_range_used(candidate_region, owner) {
                let start = PhysicalFrameStart::new(PhysAddr::new(candidate_region.start))
                    .expect("frame allocator returned an unaligned physical frame");
                return PhysicalFrameRange::new(start, frame_count);
            }
        }

        None
    }

    /// Return owned frames to the free pool when the expected owner matches.
    pub fn free_frames_for(&mut self, range: PhysicalFrameRange, owner: FrameRangeOwner) -> bool {
        if owner == FrameRangeOwner::Free {
            return false;
        }

        self.mark_range_free(
            Region {
                start: range.start().as_u64(),
                pages: range.frame_count().as_u64(),
            },
            owner,
        )
    }

    /// Return page totals for reserved, free, and used tracked frame ranges.
    #[allow(dead_code)]
    pub fn statistics(&self) -> FrameAllocatorStatistics {
        let mut statistics = FrameAllocatorStatistics::default();
        for index in 0..self.tracked_count {
            let range = self.tracked_ranges[index];
            match range.state {
                FrameRangeState::Reserved => {
                    statistics.reserved = statistics.reserved.saturating_add(range.region.pages);
                }
                FrameRangeState::Free => {
                    statistics.free = statistics.free.saturating_add(range.region.pages);
                }
                FrameRangeState::Used => {
                    statistics.used = statistics.used.saturating_add(range.region.pages);
                }
            }
        }
        statistics
    }

    /// Return page totals grouped by tracked frame-range owner.
    #[allow(dead_code)]
    pub fn owner_statistics(&self) -> FrameAllocatorOwnerStatistics {
        let mut statistics = FrameAllocatorOwnerStatistics::default();
        for index in 0..self.tracked_count {
            let range = self.tracked_ranges[index];
            let pages = range.region.pages;
            match range.owner {
                FrameRangeOwner::Free => statistics.free = statistics.free.saturating_add(pages),
                FrameRangeOwner::FirmwareReserved => {
                    statistics.firmware_reserved =
                        statistics.firmware_reserved.saturating_add(pages);
                }
                FrameRangeOwner::KernelImage => {
                    statistics.kernel_image = statistics.kernel_image.saturating_add(pages);
                }
                FrameRangeOwner::Mmio => statistics.mmio = statistics.mmio.saturating_add(pages),
                FrameRangeOwner::GuardPage => {
                    statistics.guard_page = statistics.guard_page.saturating_add(pages);
                }
                FrameRangeOwner::UnknownUsed => {
                    statistics.unknown_used = statistics.unknown_used.saturating_add(pages);
                }
                FrameRangeOwner::PageTable => {
                    statistics.page_table = statistics.page_table.saturating_add(pages);
                }
                FrameRangeOwner::KernelHeap => {
                    statistics.kernel_heap = statistics.kernel_heap.saturating_add(pages);
                }
                FrameRangeOwner::KernelStack => {
                    statistics.kernel_stack = statistics.kernel_stack.saturating_add(pages);
                }
                FrameRangeOwner::FramebufferBackbuffer => {
                    statistics.framebuffer_backbuffer =
                        statistics.framebuffer_backbuffer.saturating_add(pages);
                }
                FrameRangeOwner::AhciDma => {
                    statistics.ahci_dma = statistics.ahci_dma.saturating_add(pages);
                }
                FrameRangeOwner::DynamicKernelMapping => {
                    statistics.dynamic_kernel_mapping =
                        statistics.dynamic_kernel_mapping.saturating_add(pages);
                }
                FrameRangeOwner::UserStack => {
                    statistics.user_stack = statistics.user_stack.saturating_add(pages);
                }
                FrameRangeOwner::UserElf => {
                    statistics.user_elf = statistics.user_elf.saturating_add(pages);
                }
                FrameRangeOwner::UserHeap => {
                    statistics.user_heap = statistics.user_heap.saturating_add(pages);
                }
                FrameRangeOwner::UserMapping => {
                    statistics.user_mapping = statistics.user_mapping.saturating_add(pages);
                }
            }
        }
        statistics
    }

    fn pages_owned_by(&self, owner: FrameRangeOwner) -> u64 {
        let mut pages = 0_u64;
        for index in 0..self.tracked_count {
            let range = self.tracked_ranges[index];
            if range.owner == owner {
                pages = pages.saturating_add(range.region.pages);
            }
        }
        pages
    }

    /// Total number of registered conventional memory in bytes.
    #[allow(dead_code)]
    pub fn total_bytes(&self) -> u64 {
        let mut total = 0;
        for i in 0..self.count {
            total += self.regions[i].pages * FRAME_SIZE;
        }
        total
    }

    fn insert_region(&mut self, region: Region) {
        if self.count == 0 {
            self.regions[0] = region;
            self.count = 1;
            return;
        }

        let mut index = 0;
        while index < self.count && self.regions[index].start < region.start {
            index += 1;
        }

        if self.count >= MAX_REGIONS {
            return;
        }

        for move_index in (index..self.count).rev() {
            self.regions[move_index + 1] = self.regions[move_index];
        }
        self.regions[index] = region;
        self.count += 1;
        self.merge_adjacent_regions();
    }

    fn merge_adjacent_regions(&mut self) {
        if self.count < 2 {
            return;
        }

        let mut write_index = 0;
        for read_index in 1..self.count {
            let current_end = region_end(self.regions[write_index]);
            if current_end == Some(self.regions[read_index].start) {
                self.regions[write_index].pages = self.regions[write_index]
                    .pages
                    .saturating_add(self.regions[read_index].pages);
            } else {
                write_index += 1;
                self.regions[write_index] = self.regions[read_index];
            }
        }

        self.count = write_index + 1;
    }

    fn insert_tracked_range(
        &mut self,
        region: Region,
        state: FrameRangeState,
        owner: FrameRangeOwner,
    ) {
        if region.pages == 0 {
            return;
        }

        let mut index = 0;
        while index < self.tracked_count && self.tracked_ranges[index].region.start < region.start {
            index += 1;
        }

        self.insert_tracked_range_at(
            index,
            TrackedRange {
                region,
                state,
                owner,
            },
        );
        self.merge_adjacent_tracked_ranges();
    }

    fn insert_tracked_range_at(&mut self, index: usize, range: TrackedRange) {
        assert!(
            self.tracked_count < MAX_TRACKED_RANGES,
            "frame allocator range tracking capacity exhausted"
        );
        assert!(
            index <= self.tracked_count,
            "frame allocator range tracking insert index out of bounds"
        );

        for move_index in (index..self.tracked_count).rev() {
            self.tracked_ranges[move_index + 1] = self.tracked_ranges[move_index];
        }
        self.tracked_ranges[index] = range;
        self.tracked_count += 1;
    }

    fn remove_tracked_range_at(&mut self, index: usize) {
        assert!(
            index < self.tracked_count,
            "frame allocator range tracking remove index out of bounds"
        );

        for move_index in index..self.tracked_count - 1 {
            self.tracked_ranges[move_index] = self.tracked_ranges[move_index + 1];
        }
        self.tracked_count -= 1;
    }

    fn remove_reserved_region_from_free_ranges(&mut self, reserved_region: Region) {
        let Some(reserved_end) = region_end(reserved_region) else {
            return;
        };

        let mut index = 0;
        while index < self.tracked_count {
            let tracked = self.tracked_ranges[index];
            if tracked.state != FrameRangeState::Free {
                index += 1;
                continue;
            }

            let Some(tracked_end) = region_end(tracked.region) else {
                index += 1;
                continue;
            };
            let overlap_start = tracked.region.start.max(reserved_region.start);
            let overlap_end = tracked_end.min(reserved_end);
            if overlap_start >= overlap_end {
                index += 1;
                continue;
            }

            let before_pages = (overlap_start - tracked.region.start) / FRAME_SIZE;
            let after_pages = (tracked_end - overlap_end) / FRAME_SIZE;
            if before_pages > 0 && after_pages > 0 {
                self.tracked_ranges[index].region.pages = before_pages;
                self.insert_tracked_range_at(
                    index + 1,
                    TrackedRange {
                        region: Region {
                            start: overlap_end,
                            pages: after_pages,
                        },
                        state: FrameRangeState::Free,
                        owner: FrameRangeOwner::Free,
                    },
                );
                index += 2;
            } else if before_pages > 0 {
                self.tracked_ranges[index].region.pages = before_pages;
                index += 1;
            } else if after_pages > 0 {
                self.tracked_ranges[index].region.start = overlap_end;
                self.tracked_ranges[index].region.pages = after_pages;
                index += 1;
            } else {
                self.remove_tracked_range_at(index);
            }
        }

        self.merge_adjacent_tracked_ranges();
    }

    fn mark_range_used(&mut self, used_region: Region, owner: FrameRangeOwner) -> bool {
        let Some(used_end) = region_end(used_region) else {
            return false;
        };

        for index in 0..self.tracked_count {
            let tracked = self.tracked_ranges[index];
            if tracked.state != FrameRangeState::Free {
                continue;
            }
            let Some(tracked_end) = region_end(tracked.region) else {
                return false;
            };
            if used_region.start < tracked.region.start || used_end > tracked_end {
                continue;
            }

            let before_pages = (used_region.start - tracked.region.start) / FRAME_SIZE;
            let after_pages = (tracked_end - used_end) / FRAME_SIZE;
            self.tracked_ranges[index] = TrackedRange {
                region: used_region,
                state: FrameRangeState::Used,
                owner,
            };

            if before_pages > 0 {
                self.tracked_ranges[index] = TrackedRange {
                    region: Region {
                        start: tracked.region.start,
                        pages: before_pages,
                    },
                    state: FrameRangeState::Free,
                    owner: FrameRangeOwner::Free,
                };
                self.insert_tracked_range_at(
                    index + 1,
                    TrackedRange {
                        region: used_region,
                        state: FrameRangeState::Used,
                        owner,
                    },
                );
                if after_pages > 0 {
                    self.insert_tracked_range_at(
                        index + 2,
                        TrackedRange {
                            region: Region {
                                start: used_end,
                                pages: after_pages,
                            },
                            state: FrameRangeState::Free,
                            owner: FrameRangeOwner::Free,
                        },
                    );
                }
            } else if after_pages > 0 {
                self.insert_tracked_range_at(
                    index + 1,
                    TrackedRange {
                        region: Region {
                            start: used_end,
                            pages: after_pages,
                        },
                        state: FrameRangeState::Free,
                        owner: FrameRangeOwner::Free,
                    },
                );
            }

            self.merge_adjacent_tracked_ranges();
            return true;
        }

        false
    }

    fn mark_range_free(&mut self, freed_region: Region, owner: FrameRangeOwner) -> bool {
        let Some(freed_end) = region_end(freed_region) else {
            return false;
        };

        for index in 0..self.tracked_count {
            let tracked = self.tracked_ranges[index];
            if tracked.state != FrameRangeState::Used || tracked.owner != owner {
                continue;
            }
            let Some(tracked_end) = region_end(tracked.region) else {
                return false;
            };
            if freed_region.start < tracked.region.start || freed_end > tracked_end {
                continue;
            }

            let before_pages = (freed_region.start - tracked.region.start) / FRAME_SIZE;
            let after_pages = (tracked_end - freed_end) / FRAME_SIZE;
            self.tracked_ranges[index] = TrackedRange {
                region: freed_region,
                state: FrameRangeState::Free,
                owner: FrameRangeOwner::Free,
            };

            if before_pages > 0 {
                self.tracked_ranges[index] = TrackedRange {
                    region: Region {
                        start: tracked.region.start,
                        pages: before_pages,
                    },
                    state: FrameRangeState::Used,
                    owner,
                };
                self.insert_tracked_range_at(
                    index + 1,
                    TrackedRange {
                        region: freed_region,
                        state: FrameRangeState::Free,
                        owner: FrameRangeOwner::Free,
                    },
                );
                if after_pages > 0 {
                    self.insert_tracked_range_at(
                        index + 2,
                        TrackedRange {
                            region: Region {
                                start: freed_end,
                                pages: after_pages,
                            },
                            state: FrameRangeState::Used,
                            owner,
                        },
                    );
                }
            } else if after_pages > 0 {
                self.insert_tracked_range_at(
                    index + 1,
                    TrackedRange {
                        region: Region {
                            start: freed_end,
                            pages: after_pages,
                        },
                        state: FrameRangeState::Used,
                        owner,
                    },
                );
            }

            self.merge_adjacent_tracked_ranges();
            return true;
        }

        false
    }

    fn merge_adjacent_tracked_ranges(&mut self) {
        if self.tracked_count < 2 {
            return;
        }

        let mut write_index = 0;
        for read_index in 1..self.tracked_count {
            let current = self.tracked_ranges[write_index];
            let next = self.tracked_ranges[read_index];
            if current.state == next.state
                && current.owner == next.owner
                && region_end(current.region) == Some(next.region.start)
            {
                self.tracked_ranges[write_index].region.pages = self.tracked_ranges[write_index]
                    .region
                    .pages
                    .saturating_add(next.region.pages);
            } else {
                write_index += 1;
                self.tracked_ranges[write_index] = next;
            }
        }

        self.tracked_count = write_index + 1;
    }
}

fn normalize_region(start: PhysAddr, frame_count: FrameCount) -> Option<Region> {
    let start = start.as_u64();
    let byte_count = frame_count.byte_len();
    let end = start.checked_add(byte_count)?;
    let aligned_start = align_up(start.max(FRAME_SIZE), FRAME_SIZE)?;
    if aligned_start >= end {
        return None;
    }

    Some(Region {
        start: aligned_start,
        pages: (end - aligned_start) / FRAME_SIZE,
    })
}

fn normalize_reserved_region(start: PhysAddr, frame_count: FrameCount) -> Option<Region> {
    let start = start.as_u64();
    let byte_count = frame_count.byte_len();
    let end = start.checked_add(byte_count)?;
    let aligned_start = align_up(start, FRAME_SIZE)?;
    if aligned_start >= end {
        return None;
    }

    Some(Region {
        start: aligned_start,
        pages: (end - aligned_start) / FRAME_SIZE,
    })
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let mask = alignment.checked_sub(1)?;
    value.checked_add(mask).map(|value| value & !mask)
}

fn region_end(region: Region) -> Option<u64> {
    region
        .pages
        .checked_mul(FRAME_SIZE)
        .and_then(|byte_count| region.start.checked_add(byte_count))
}
