//! Typed memory address wrappers.

const PAGE_SIZE: u64 = 4096;
const KERNEL_DYNAMIC_MAPPING_START: u64 = 0xffff_8000_0000_0000;
const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;

/// A raw physical byte address kept distinct from virtual addresses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysAddr(u64);

impl PhysAddr {
    /// Create a physical byte address.
    pub const fn new(address: u64) -> Self {
        Self(address)
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
        usize::try_from(self.0).expect("physical address must fit in usize")
    }

    /// Return a physical address advanced by `offset` bytes.
    pub const fn checked_add(self, offset: u64) -> Option<Self> {
        let Some(address) = self.0.checked_add(offset) else {
            return None;
        };
        Some(Self(address))
    }

    /// Return this physical address rounded down to a 4 KiB page boundary.
    pub const fn align_down_to_page(self) -> Self {
        Self(self.0 & !(PAGE_SIZE - 1))
    }
}

/// A physical byte address owned by a DMA-capable device descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaPhysicalAddress(PhysAddr);

impl DmaPhysicalAddress {
    /// Create a DMA physical address from an owned physical address.
    pub const fn new(address: PhysAddr) -> Self {
        Self(address)
    }

    /// Return the raw DMA physical address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0.as_u64()
    }

    /// Return the raw DMA physical address as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if the DMA physical address does not fit in `usize`.
    pub fn as_usize(self) -> usize {
        self.0.as_usize()
    }
}

/// A physical DMA data buffer address used by storage parsers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageDataAddress(DmaPhysicalAddress);

impl StorageDataAddress {
    /// Create a storage data-buffer address from a DMA physical address.
    pub const fn new(address: DmaPhysicalAddress) -> Self {
        Self(address)
    }

    /// Return the raw storage data-buffer address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0.as_u64()
    }

    /// Return the raw storage data-buffer address as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if the storage data-buffer address does not fit in `usize`.
    pub fn as_usize(self) -> usize {
        self.0.as_usize()
    }
}

/// A raw virtual byte address kept distinct from physical addresses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtAddr(u64);

impl VirtAddr {
    /// Create a virtual byte address.
    pub const fn new(address: u64) -> Self {
        Self(address)
    }

    /// Return the raw virtual address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Return a virtual address advanced by `offset` bytes.
    pub const fn checked_add(self, offset: u64) -> Option<Self> {
        let Some(address) = self.0.checked_add(offset) else {
            return None;
        };
        Some(Self(address))
    }

    /// Return this virtual address rounded down to a 4 KiB page boundary.
    pub const fn align_down_to_page(self) -> Self {
        Self(self.0 & !(PAGE_SIZE - 1))
    }
}

/// A mapped kernel virtual byte address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelVirtualAddress(VirtAddr);

impl KernelVirtualAddress {
    /// Create a mapped kernel virtual address.
    pub const fn new(address: VirtAddr) -> Self {
        Self(address)
    }

    /// Return the raw kernel virtual address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0.as_u64()
    }

    /// Return the kernel virtual address as a mutable byte pointer.
    ///
    /// # Panics
    ///
    /// Panics if the kernel virtual address does not fit in `usize`.
    pub fn as_mut_ptr(self) -> *mut u8 {
        usize::try_from(self.as_u64()).expect("kernel virtual address must fit in usize") as *mut u8
    }
}

/// A reserved kernel virtual range for future dynamic mappings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelVirtualRange {
    start: VirtAddr,
    page_count: u64,
}

impl KernelVirtualRange {
    /// Create a non-empty 4 KiB-aligned kernel virtual range.
    pub const fn new(start: VirtAddr, page_count: u64) -> Option<Self> {
        if page_count == 0
            || !start.as_u64().is_multiple_of(PAGE_SIZE)
            || start.as_u64() < KERNEL_DYNAMIC_MAPPING_START
        {
            return None;
        }

        let Some(byte_len) = page_count.checked_mul(PAGE_SIZE) else {
            return None;
        };
        let Some(_) = start.as_u64().checked_add(byte_len) else {
            return None;
        };

        Some(Self { start, page_count })
    }

    /// Return the first virtual address in the range.
    pub const fn start(self) -> VirtAddr {
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
            .expect("kernel virtual range byte length overflowed")
    }

    /// Return the first virtual address after this range.
    ///
    /// # Panics
    ///
    /// Panics if the range end overflows `u64`. Construction prevents this for
    /// valid ranges.
    pub const fn end_exclusive(self) -> VirtAddr {
        self.start
            .checked_add(self.byte_len())
            .expect("kernel virtual range end overflowed")
    }
}

/// A physical framebuffer range reported by the active graphics mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramebufferPhysicalRange {
    start: PhysAddr,
    byte_len: u64,
}

impl FramebufferPhysicalRange {
    /// Create a non-empty framebuffer physical range.
    pub const fn new(start: PhysAddr, byte_len: u64) -> Option<Self> {
        if byte_len == 0 {
            return None;
        }

        Some(Self { start, byte_len })
    }

    /// Return the framebuffer physical base address.
    pub const fn start(self) -> PhysAddr {
        self.start
    }

    /// Return the framebuffer range length in bytes.
    pub const fn byte_len(self) -> u64 {
        self.byte_len
    }
}

/// A 4 KiB-aligned physical frame start address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalFrameStart(PhysAddr);

impl PhysicalFrameStart {
    /// Create a physical frame start address if `address` is 4 KiB-aligned.
    pub const fn new(address: PhysAddr) -> Option<Self> {
        if address.as_u64().is_multiple_of(PAGE_SIZE) {
            Some(Self(address))
        } else {
            None
        }
    }

    /// Return the raw physical address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0.as_u64()
    }

    /// Return this frame start as a physical byte address.
    pub const fn as_address(self) -> PhysAddr {
        self.0
    }

    /// Return this identity-mapped frame start as a kernel virtual address.
    pub const fn as_identity_mapped_kernel_address(self) -> KernelVirtualAddress {
        KernelVirtualAddress::new(VirtAddr::new(self.0.as_u64()))
    }

    /// Return this frame start as a DMA physical byte address.
    pub const fn as_dma_address(self) -> DmaPhysicalAddress {
        DmaPhysicalAddress::new(self.as_address())
    }

    /// Return the raw physical address as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if the physical address does not fit in `usize`.
    pub fn as_usize(self) -> usize {
        self.0.as_usize()
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
pub struct UserVirtualAddress(VirtAddr);

impl UserVirtualAddress {
    /// Create a user virtual address if `address` is non-zero and below the
    /// user-space ceiling.
    pub const fn new(address: VirtAddr) -> Option<Self> {
        if address.as_u64() != 0 && address.as_u64() < USER_SPACE_END {
            Some(Self(address))
        } else {
            None
        }
    }

    /// Return the raw virtual address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0.as_u64()
    }

    /// Return the raw virtual address as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if the user virtual address does not fit in `usize`.
    pub fn as_usize(self) -> usize {
        usize::try_from(self.0.as_u64()).expect("user virtual address must fit in usize")
    }

    /// Return a user virtual address advanced by `offset` bytes.
    pub const fn checked_add(self, offset: u64) -> Option<Self> {
        let Some(address) = self.0.checked_add(offset) else {
            return None;
        };
        Self::new(address)
    }

    /// Return this user virtual address rounded down to a 4 KiB page boundary.
    pub const fn align_down_to_page(self) -> Option<UserPageStart> {
        let Some(address) = Self::new(self.0.align_down_to_page()) else {
            return None;
        };
        UserPageStart::new(address)
    }

    /// Return a user virtual address moved backward by `offset` bytes.
    pub const fn checked_sub(self, offset: u64) -> Option<Self> {
        let Some(address) = self.0.as_u64().checked_sub(offset) else {
            return None;
        };
        Self::new(VirtAddr::new(address))
    }
}

/// A 4 KiB-aligned user virtual page start address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserPageStart(UserVirtualAddress);

impl UserPageStart {
    /// Create a user page start address if `address` is 4 KiB-aligned.
    pub const fn new(address: UserVirtualAddress) -> Option<Self> {
        if address.as_u64().is_multiple_of(PAGE_SIZE) {
            Some(Self(address))
        } else {
            None
        }
    }

    /// Return this page start as a user virtual address.
    pub const fn as_address(self) -> UserVirtualAddress {
        self.0
    }

    /// Return the raw virtual address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0.as_u64()
    }

    /// Return a user page start address advanced by `offset` bytes.
    pub const fn checked_add(self, offset: u64) -> Option<Self> {
        let Some(address) = self.0.checked_add(offset) else {
            return None;
        };
        Self::new(address)
    }

    /// Return a user page start address moved backward by `offset` bytes.
    pub const fn checked_sub(self, offset: u64) -> Option<Self> {
        let Some(address) = self.0.checked_sub(offset) else {
            return None;
        };
        Self::new(address)
    }
}

/// Verify the typed user virtual address construction contract.
pub fn verify_typed_user_virtual_address() -> bool {
    let valid_user_address = UserVirtualAddress::new(VirtAddr::new(PAGE_SIZE));
    let zero_user_address = UserVirtualAddress::new(VirtAddr::new(0));
    let ceiling_user_address = UserVirtualAddress::new(VirtAddr::new(USER_SPACE_END));

    valid_user_address.is_some_and(|address| {
        address.as_u64() == PAGE_SIZE
            && address
                .align_down_to_page()
                .is_some_and(|page_start| page_start.as_u64() == PAGE_SIZE)
            && address
                .checked_add(PAGE_SIZE)
                .is_some_and(|next_address| next_address.as_u64() == 2 * PAGE_SIZE)
    }) && zero_user_address.is_none()
        && ceiling_user_address.is_none()
}

/// Verify the typed user page start construction contract.
pub fn verify_typed_user_page_start() -> bool {
    let aligned_address = UserVirtualAddress::new(VirtAddr::new(PAGE_SIZE));
    let unaligned_address = UserVirtualAddress::new(VirtAddr::new(PAGE_SIZE + 1));
    let low_address = UserVirtualAddress::new(VirtAddr::new(1));

    aligned_address.is_some_and(|address| {
        UserPageStart::new(address).is_some_and(|page_start| {
            page_start.as_address() == address
                && page_start.as_u64() == PAGE_SIZE
                && page_start
                    .checked_add(PAGE_SIZE)
                    .is_some_and(|next_page| next_page.as_u64() == 2 * PAGE_SIZE)
        })
    }) && unaligned_address.is_some_and(|address| UserPageStart::new(address).is_none())
        && low_address.is_some_and(|address| address.align_down_to_page().is_none())
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
        let start = UserVirtualAddress::new(VirtAddr::new(user_pointer))?;
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

    /// Return the first address after this range.
    ///
    /// # Panics
    ///
    /// Panics if the range end overflows `u64`. Construction prevents this for
    /// valid ranges.
    pub const fn end_exclusive(self) -> u64 {
        self.start
            .as_u64()
            .checked_add(self.byte_len as u64)
            .expect("user virtual range end overflowed")
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

/// A readable user range intended to contain a NUL-terminated C string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserCString(UserReadableRange);

impl UserCString {
    /// Create a user C-string candidate from a readable user range.
    pub const fn new(range: UserReadableRange) -> Self {
        Self(range)
    }

    /// Return the underlying readable user range.
    pub const fn as_readable_range(self) -> UserReadableRange {
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
