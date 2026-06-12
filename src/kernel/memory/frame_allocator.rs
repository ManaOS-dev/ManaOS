//! A simple bump physical frame allocator.

use super::address::{PhysicalFrameRange, PhysicalFrameStart};

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

/// A linear allocator for 4 KiB physical frames registered from UEFI memory map
/// regions.
pub struct BumpFrameAllocator {
    regions: [Region; MAX_REGIONS],
    count: usize,
    current: usize,
    offset: u64,
    tracked_ranges: [TrackedRange; MAX_TRACKED_RANGES],
    tracked_count: usize,
}

impl BumpFrameAllocator {
    /// Create an empty physical frame allocator.
    pub const fn new() -> Self {
        Self {
            regions: [Region { start: 0, pages: 0 }; MAX_REGIONS],
            count: 0,
            current: 0,
            offset: 0,
            tracked_ranges: [TrackedRange {
                region: Region { start: 0, pages: 0 },
                state: FrameRangeState::Reserved,
            }; MAX_TRACKED_RANGES],
            tracked_count: 0,
        }
    }

    /// Register a conventional memory region (call before `ExitBootServices`).
    pub fn add_region(&mut self, start: u64, pages: u64) {
        let Some(region) = normalize_region(start, pages) else {
            return;
        };

        self.insert_region(region);
        self.insert_tracked_range(region, FrameRangeState::Free);
    }

    /// Register a reserved physical memory region.
    pub fn reserve_region(&mut self, start: u64, pages: u64) {
        let Some(region) = normalize_reserved_region(start, pages) else {
            return;
        };

        self.insert_tracked_range(region, FrameRangeState::Reserved);
        self.remove_reserved_region_from_free_ranges(region);
    }

    /// Allocate a single 4KiB frame.
    #[allow(dead_code)]
    pub fn allocate_frame(&mut self) -> Option<PhysicalFrameStart> {
        self.allocate_frames(1).map(PhysicalFrameRange::start)
    }

    /// Allocate `n` contiguous 4KiB frames.
    /// Contiguous allocation is only guaranteed within a single region.
    pub fn allocate_frames(&mut self, n: u64) -> Option<PhysicalFrameRange> {
        if n == 0 {
            return None;
        }

        while self.current < self.count {
            let region = &self.regions[self.current];
            let available_pages = region.pages.saturating_sub(self.offset);
            if available_pages >= n {
                let Some(candidate_offset) = self.offset.checked_mul(FRAME_SIZE) else {
                    self.current += 1;
                    self.offset = 0;
                    continue;
                };
                let Some(candidate_address) = region.start.checked_add(candidate_offset) else {
                    self.current += 1;
                    self.offset = 0;
                    continue;
                };

                if !self.mark_range_used(Region {
                    start: candidate_address,
                    pages: n,
                }) {
                    self.offset = self.offset.saturating_add(1);
                    continue;
                }
                self.offset += n;
                let start = PhysicalFrameStart::new(candidate_address)
                    .expect("frame allocator returned an unaligned physical frame");
                return Some(
                    PhysicalFrameRange::new(start, n)
                        .expect("frame allocator returned an empty physical frame range"),
                );
            }
            // Move to the next region
            self.current += 1;
            self.offset = 0;
        }
        None
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

    fn insert_tracked_range(&mut self, region: Region, state: FrameRangeState) {
        if region.pages == 0 {
            return;
        }

        let mut index = 0;
        while index < self.tracked_count && self.tracked_ranges[index].region.start < region.start {
            index += 1;
        }

        self.insert_tracked_range_at(index, TrackedRange { region, state });
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

    fn mark_range_used(&mut self, used_region: Region) -> bool {
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
            };

            if before_pages > 0 {
                self.tracked_ranges[index] = TrackedRange {
                    region: Region {
                        start: tracked.region.start,
                        pages: before_pages,
                    },
                    state: FrameRangeState::Free,
                };
                self.insert_tracked_range_at(
                    index + 1,
                    TrackedRange {
                        region: used_region,
                        state: FrameRangeState::Used,
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
            if current.state == next.state && region_end(current.region) == Some(next.region.start)
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

fn normalize_region(start: u64, pages: u64) -> Option<Region> {
    let byte_count = pages.checked_mul(FRAME_SIZE)?;
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

fn normalize_reserved_region(start: u64, pages: u64) -> Option<Region> {
    let byte_count = pages.checked_mul(FRAME_SIZE)?;
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

/// Verify the frame-zero skip behavior for multi-frame allocations.
#[allow(dead_code)]
pub fn verify_zero_address_skip_for_multi_frame_allocations() -> bool {
    let mut frame_allocator = BumpFrameAllocator::new();
    frame_allocator.add_region(0, 3);

    frame_allocator
        .allocate_frames(2)
        .map(|range| range.start().as_u64())
        == Some(FRAME_SIZE)
}

/// Verify reserved, used, and free frame range tracking.
#[allow(dead_code)]
pub fn verify_reserved_used_and_free_range_tracking() -> bool {
    let mut frame_allocator = BumpFrameAllocator::new();
    frame_allocator.reserve_region(0, 1);
    frame_allocator.add_region(FRAME_SIZE, 4);

    if frame_allocator.allocate_frames(2).is_none() {
        return false;
    }

    frame_allocator.statistics()
        == FrameAllocatorStatistics {
            reserved: 1,
            free: 2,
            used: 2,
        }
}

/// Verify that allocations never return the same physical frame twice.
#[allow(dead_code)]
pub fn verify_duplicate_allocation_rejection() -> bool {
    let mut frame_allocator = BumpFrameAllocator::new();
    frame_allocator.add_region(0, 4);

    let Some(first_frame) = frame_allocator.allocate_frame() else {
        return false;
    };
    let Some(second_frame) = frame_allocator.allocate_frame() else {
        return false;
    };

    first_frame != second_frame
        && frame_allocator.statistics()
            == FrameAllocatorStatistics {
                reserved: 0,
                free: 1,
                used: 2,
            }
}

/// Verify that contiguous allocations do not cross registered region gaps.
#[allow(dead_code)]
pub fn verify_contiguous_allocation_boundaries() -> bool {
    let mut frame_allocator = BumpFrameAllocator::new();
    frame_allocator.add_region(FRAME_SIZE, 1);
    frame_allocator.add_region(3 * FRAME_SIZE, 2);

    frame_allocator
        .allocate_frames(2)
        .map(|range| range.start().as_u64())
        == Some(3 * FRAME_SIZE)
}

/// Verify that reserved ranges inside a free region are not allocated.
#[allow(dead_code)]
pub fn verify_reserved_range_exclusion() -> bool {
    let mut frame_allocator = BumpFrameAllocator::new();
    frame_allocator.add_region(FRAME_SIZE, 4);
    frame_allocator.reserve_region(2 * FRAME_SIZE, 1);

    let Some(first_frame) = frame_allocator.allocate_frame() else {
        return false;
    };
    let Some(second_frame) = frame_allocator.allocate_frame() else {
        return false;
    };

    first_frame.as_u64() == FRAME_SIZE
        && second_frame.as_u64() == 3 * FRAME_SIZE
        && frame_allocator.statistics()
            == FrameAllocatorStatistics {
                reserved: 1,
                free: 1,
                used: 2,
            }
}
