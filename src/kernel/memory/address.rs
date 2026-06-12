//! Typed memory address wrappers.

const PAGE_SIZE: u64 = 4096;

/// A 4 KiB-aligned physical frame start address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalFrameStart(u64);

impl PhysicalFrameStart {
    /// Create a physical frame start address if `address` is 4 KiB-aligned.
    pub const fn new(address: u64) -> Option<Self> {
        if address.is_multiple_of(PAGE_SIZE) {
            Some(Self(address))
        } else {
            None
        }
    }

    /// Return the raw physical address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Return the raw physical address as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if the physical address does not fit in `usize`.
    pub fn as_usize(self) -> usize {
        usize::try_from(self.0).expect("physical frame address must fit in usize")
    }
}

/// A contiguous owned range of 4 KiB physical frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalFrameRange {
    start: PhysicalFrameStart,
    page_count: u64,
}

impl PhysicalFrameRange {
    /// Create a physical frame range from a start address and page count.
    pub const fn new(start: PhysicalFrameStart, page_count: u64) -> Option<Self> {
        if page_count == 0 {
            return None;
        }

        Some(Self { start, page_count })
    }

    /// Return the first physical frame in the range.
    pub const fn start(self) -> PhysicalFrameStart {
        self.start
    }

    /// Return the number of 4 KiB pages in the range.
    pub const fn page_count(self) -> u64 {
        self.page_count
    }

    /// Return the byte length of the range.
    ///
    /// # Panics
    ///
    /// Panics if `page_count * 4096` overflows `u64`.
    pub const fn byte_len(self) -> u64 {
        self.page_count
            .checked_mul(PAGE_SIZE)
            .expect("physical frame range byte length overflowed")
    }
}
