/// A simple bump (linear) physical frame allocator.
/// Conventional memory regions are registered before `ExitBootServices`,
/// and physical frames (4KB units) are allocated after `ExitBootServices`.
const MAX_REGIONS: usize = 128;

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
            let r = &self.regions[self.current];
            let avail = r.pages.saturating_sub(self.offset);
            if avail >= n {
                let addr = r.start + self.offset * 4096;

                // Never allocate address 0
                if addr == 0 {
                    self.offset += 1;
                    if avail > n {
                        continue;
                    }
                    self.current += 1;
                    self.offset = 0;
                    continue;
                }

                self.offset += n;
                return Some(addr);
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
            total += self.regions[i].pages * 4096;
        }
        total
    }
}
