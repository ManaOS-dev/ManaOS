//! A simple bump physical frame allocator.

use super::address::PhysicalFrameStart;

const MAX_REGIONS: usize = 128;
const FRAME_SIZE: u64 = 4096;

#[derive(Clone, Copy)]
struct Region {
    start: u64,
    pages: u64,
}

/// A linear allocator for 4 KiB physical frames registered from UEFI memory map
/// regions.
pub struct BumpFrameAllocator {
    regions: [Region; MAX_REGIONS],
    count: usize,
    current: usize,
    offset: u64,
}

impl BumpFrameAllocator {
    /// Create an empty physical frame allocator.
    pub const fn new() -> Self {
        Self {
            regions: [Region { start: 0, pages: 0 }; MAX_REGIONS],
            count: 0,
            current: 0,
            offset: 0,
        }
    }

    /// Register a conventional memory region (call before `ExitBootServices`).
    pub fn add_region(&mut self, start: u64, pages: u64) {
        let Some(region) = normalize_region(start, pages) else {
            return;
        };

        self.insert_region(region);
    }

    /// Allocate a single 4KiB frame.
    #[allow(dead_code)]
    pub fn allocate_frame(&mut self) -> Option<PhysicalFrameStart> {
        self.allocate_frames(1)
    }

    /// Allocate `n` contiguous 4KiB frames.
    /// Contiguous allocation is only guaranteed within a single region.
    pub fn allocate_frames(&mut self, n: u64) -> Option<PhysicalFrameStart> {
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

                self.offset += n;
                return Some(
                    PhysicalFrameStart::new(candidate_address)
                        .expect("frame allocator returned an unaligned physical frame"),
                );
            }
            // Move to the next region
            self.current += 1;
            self.offset = 0;
        }
        None
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
        .map(PhysicalFrameStart::as_u64)
        == Some(FRAME_SIZE)
}
