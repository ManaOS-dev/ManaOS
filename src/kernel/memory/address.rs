//! Typed memory address wrappers.

const PAGE_SIZE: u64 = 4096;
const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;

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

/// A non-null virtual address in the user half of the address space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserVirtualAddress(u64);

impl UserVirtualAddress {
    /// Create a user virtual address if `address` is non-zero and below the
    /// user-space ceiling.
    pub const fn new(address: u64) -> Option<Self> {
        if address != 0 && address < USER_SPACE_END {
            Some(Self(address))
        } else {
            None
        }
    }

    /// Return the raw virtual address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Return the raw virtual address as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if the user virtual address does not fit in `usize`.
    pub fn as_usize(self) -> usize {
        usize::try_from(self.0).expect("user virtual address must fit in usize")
    }

    /// Return a user virtual address moved backward by `offset` bytes.
    pub const fn checked_sub(self, offset: u64) -> Option<Self> {
        let Some(address) = self.0.checked_sub(offset) else {
            return None;
        };
        Self::new(address)
    }
}

/// A non-empty byte range fully contained in user virtual address space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserVirtualRange {
    start: UserVirtualAddress,
    byte_len: usize,
}

impl UserVirtualRange {
    /// Create a non-empty user virtual byte range.
    pub fn new(start: UserVirtualAddress, byte_len: usize) -> Option<Self> {
        if byte_len == 0 {
            return None;
        }

        let byte_len_u64 = u64::try_from(byte_len).ok()?;
        let end = start.as_u64().checked_add(byte_len_u64)?;
        if end > USER_SPACE_END {
            return None;
        }

        Some(Self { start, byte_len })
    }

    /// Convert raw syscall ABI pointer and length arguments into a user range.
    pub fn from_syscall_arguments(user_pointer: u64, byte_len: u64) -> Option<Self> {
        let byte_len = usize::try_from(byte_len).ok()?;
        let start = UserVirtualAddress::new(user_pointer)?;
        Self::new(start, byte_len)
    }

    /// Return the first address in the range.
    pub const fn start(self) -> UserVirtualAddress {
        self.start
    }

    /// Return the range length in bytes.
    pub const fn byte_len(self) -> usize {
        self.byte_len
    }
}

/// A user virtual range intended for kernel reads from user memory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserReadableRange(UserVirtualRange);

impl UserReadableRange {
    /// Create a readable user range from a validated user virtual range.
    pub const fn new(range: UserVirtualRange) -> Self {
        Self(range)
    }

    /// Return the underlying user virtual range.
    pub const fn as_range(self) -> UserVirtualRange {
        self.0
    }
}

/// A user virtual range intended for kernel writes to user memory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserWritableRange(UserVirtualRange);

impl UserWritableRange {
    /// Create a writable user range from a validated user virtual range.
    pub const fn new(range: UserVirtualRange) -> Self {
        Self(range)
    }

    /// Return the underlying user virtual range.
    pub const fn as_range(self) -> UserVirtualRange {
        self.0
    }
}
