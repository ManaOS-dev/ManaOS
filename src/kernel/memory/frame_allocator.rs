/// A simple bump (linear) physical frame allocator.
/// Conventional memory regions are registered before `ExitBootServices`,
/// and physical frames (4KB units) are allocated after `ExitBootServices`.
const MAX_REGIONS: usize = 128;
const FRAME_SIZE: u64 = 4096;

#[derive(Clone, Copy)]
struct Region {
    start: u64, // Physical address (4KB aligned)
    pages: u64,
}

pub struct BumpFrameAllocator {
    regions: [Region; MAX_REGIONS],
    count: usize,
    current: usize, // Current region index being processed
    offset: u64,    // Number of pages already used in the current region
}

impl BumpFrameAllocator {
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
        if self.count < MAX_REGIONS {
            self.regions[self.count] = Region { start, pages };
            self.count += 1;
        }
    }

    /// Allocate a single 4KiB frame.
    #[allow(dead_code)]
    pub fn allocate_frame(&mut self) -> Option<u64> {
        self.allocate_frames(1)
    }

    /// Allocate `n` contiguous 4KiB frames.
    /// Contiguous allocation is only guaranteed within a single region.
    pub fn allocate_frames(&mut self, n: u64) -> Option<u64> {
        if n == 0 {
            return None;
        }

        while self.current < self.count {
            let region = &self.regions[self.current];
            if region.start == 0 && self.offset == 0 {
                self.offset = 1;
                continue;
            }

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
                return Some(candidate_address);
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
}

/// Verify the frame-zero skip behavior for multi-frame allocations.
#[allow(dead_code)]
pub fn verify_zero_address_skip_for_multi_frame_allocations() -> bool {
    let mut frame_allocator = BumpFrameAllocator::new();
    frame_allocator.add_region(0, 3);

    frame_allocator.allocate_frames(2) == Some(FRAME_SIZE)
}
