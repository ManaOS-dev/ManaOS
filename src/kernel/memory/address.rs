//! Typed memory address wrappers.

const PAGE_SIZE: u64 = 4096;
const KERNEL_DYNAMIC_MAPPING_START: u64 = 0xffff_8000_0000_0000;
const USER_SPACE_END: u64 = 0x0000_8000_0000_0000;

/// Failure returned when a typed address cannot lower to a host-width integer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressConversionError {
    /// The address does not fit in `usize` on the current target.
    DoesNotFitUsize,
}

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

    /// Try to return the raw physical address as a `usize`.
    pub fn try_as_usize(self) -> Result<usize, AddressConversionError> {
        usize::try_from(self.0).map_err(|_| AddressConversionError::DoesNotFitUsize)
    }

    /// Return the raw physical address as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if the physical address does not fit in `usize`.
    pub fn as_usize(self) -> usize {
        self.try_as_usize()
            .expect("physical address must fit in usize")
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

    /// Try to return the raw DMA physical address as a `usize`.
    pub fn try_as_usize(self) -> Result<usize, AddressConversionError> {
        self.0.try_as_usize()
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

    /// Try to return the raw storage data-buffer address as a `usize`.
    pub fn try_as_usize(self) -> Result<usize, AddressConversionError> {
        self.0.try_as_usize()
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

    /// Try to return the raw virtual address as a `usize`.
    pub fn try_as_usize(self) -> Result<usize, AddressConversionError> {
        usize::try_from(self.0).map_err(|_| AddressConversionError::DoesNotFitUsize)
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

    /// Try to return the raw kernel virtual address as a `usize`.
    pub fn try_as_usize(self) -> Result<usize, AddressConversionError> {
        self.0.try_as_usize()
    }

    /// Return the kernel virtual address as a mutable byte pointer.
    ///
    /// # Panics
    ///
    /// Panics if the kernel virtual address does not fit in `usize`.
    pub fn as_mut_ptr(self) -> *mut u8 {
        self.try_as_usize()
            .expect("kernel virtual address must fit in usize") as *mut u8
    }
}

/// A non-zero count of 4 KiB pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageCount(u64);

impl PageCount {
    /// Create a non-zero page count with a representable byte length.
    pub const fn new(count: u64) -> Option<Self> {
        if count == 0 || count > u64::MAX / PAGE_SIZE {
            None
        } else {
            Some(Self(count))
        }
    }

    /// Return the raw page count as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Return the byte length represented by this page count.
    pub const fn byte_len(self) -> u64 {
        self.0 * PAGE_SIZE
    }
}

/// A reserved kernel virtual range for future dynamic mappings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelVirtualRange {
    start: VirtAddr,
    page_count: PageCount,
}

impl KernelVirtualRange {
    /// Create a non-empty 4 KiB-aligned kernel virtual range.
    pub const fn new(start: VirtAddr, page_count: PageCount) -> Option<Self> {
        if !start.as_u64().is_multiple_of(PAGE_SIZE)
            || start.as_u64() < KERNEL_DYNAMIC_MAPPING_START
        {
            return None;
        }

        let Some(_) = start.as_u64().checked_add(page_count.byte_len()) else {
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
        self.page_count.as_u64()
    }

    /// Return the byte length of the range.
    pub const fn byte_len(self) -> u64 {
        self.page_count.byte_len()
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

    /// Try to return the raw physical address as a `usize`.
    pub fn try_as_usize(self) -> Result<usize, AddressConversionError> {
        self.0.try_as_usize()
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

/// A non-zero count of 4 KiB physical frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameCount(u64);

impl FrameCount {
    /// Create a non-zero frame count with a representable byte length.
    pub const fn new(count: u64) -> Option<Self> {
        if count == 0 || count > u64::MAX / PAGE_SIZE {
            None
        } else {
            Some(Self(count))
        }
    }

    /// Return the raw frame count as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Return the byte length represented by this frame count.
    pub const fn byte_len(self) -> u64 {
        self.0 * PAGE_SIZE
    }
}

/// A contiguous owned range of 4 KiB physical frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalFrameRange {
    start: PhysicalFrameStart,
    frame_count: FrameCount,
}

impl PhysicalFrameRange {
    /// Create a physical frame range from a start address and frame count.
    pub const fn new(start: PhysicalFrameStart, frame_count: FrameCount) -> Option<Self> {
        let Some(_) = start.as_u64().checked_add(frame_count.byte_len()) else {
            return None;
        };

        Some(Self { start, frame_count })
    }

    /// Return the first physical frame in the range.
    pub const fn start(self) -> PhysicalFrameStart {
        self.start
    }

    /// Return the number of 4 KiB pages in the range.
    pub const fn page_count(self) -> u64 {
        self.frame_count.as_u64()
    }

    /// Return the number of 4 KiB frames in the range.
    pub const fn frame_count(self) -> FrameCount {
        self.frame_count
    }

    /// Return the byte length of the range.
    ///
    pub const fn byte_len(self) -> u64 {
        self.frame_count.byte_len()
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

    /// Try to return the raw virtual address as a `usize`.
    pub fn try_as_usize(self) -> Result<usize, AddressConversionError> {
        self.0.try_as_usize()
    }

    /// Return this user virtual address as a virtual byte address.
    pub const fn as_address(self) -> VirtAddr {
        self.0
    }

    /// Try to return the raw virtual address as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if the user virtual address does not fit in `usize`.
    pub fn as_usize(self) -> usize {
        self.try_as_usize()
            .expect("user virtual address must fit in usize")
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

    /// Return the raw virtual address as a `usize`.
    pub fn try_as_usize(self) -> Result<usize, AddressConversionError> {
        self.0.try_as_usize()
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
            && address.as_address() == VirtAddr::new(PAGE_SIZE)
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

/// Verify the typed user virtual range and copy-direction wrapper contracts.
pub fn verify_typed_user_virtual_range() -> bool {
    let start = UserVirtualAddress::new(VirtAddr::new(PAGE_SIZE))
        .expect("test user virtual range start must be valid");
    let valid_range = UserVirtualRange::new(start, 8);
    let zero_length_rejected = UserVirtualRange::new(start, 0).is_none();
    let ceiling_start = UserVirtualAddress::new(VirtAddr::new(USER_SPACE_END - 1))
        .expect("last user byte address must be valid");
    let overflow_rejected = UserVirtualRange::new(ceiling_start, 2).is_none();
    let syscall_range = UserVirtualRange::from_syscall_arguments(PAGE_SIZE, 4);
    valid_range.is_some_and(|range| {
        let readable_range = UserReadableRange::new(range);
        let writable_range = UserWritableRange::new(range);
        range.start() == start
            && range.byte_len() == 8
            && range.end_exclusive() == PAGE_SIZE + 8
            && readable_range.as_range() == range
            && writable_range.as_range() == range
    }) && zero_length_rejected
        && overflow_rejected
        && syscall_range.is_some_and(|range| {
            range.start() == start
                && range.byte_len() == 4
                && range.end_exclusive() == PAGE_SIZE + 4
        })
}

/// Verify typed user virtual range page-boundary helper contracts.
pub fn verify_typed_user_virtual_range_page_bounds() -> bool {
    let start = UserVirtualAddress::new(VirtAddr::new(PAGE_SIZE))
        .expect("test user virtual range start must be valid");
    let Some(byte_len) = usize::try_from(PAGE_SIZE)
        .ok()
        .and_then(|page_size| page_size.checked_add(8))
    else {
        return false;
    };
    let valid_range = UserVirtualRange::new(start, byte_len);
    let low_start = UserVirtualAddress::new(VirtAddr::new(1))
        .expect("low user virtual range start must be valid");
    let low_range = UserVirtualRange::new(low_start, 1);

    valid_range.is_some_and(|range| {
        range
            .first_page_start()
            .is_some_and(|page_start| page_start.as_u64() == PAGE_SIZE)
            && range
                .last_page_start()
                .is_some_and(|page_start| page_start.as_u64() == 2 * PAGE_SIZE)
    }) && low_range.is_some_and(|range| {
        range.first_page_start().is_none() && range.last_page_start().is_none()
    })
}

/// Verify typed syscall copy-direction range constructor contracts.
pub fn verify_typed_user_copy_ranges() -> bool {
    let valid_start = PAGE_SIZE;
    let readable_range = UserReadableRange::from_syscall_arguments(valid_start, 4);
    let writable_range = UserWritableRange::from_syscall_arguments(valid_start, 4);
    let string_range = UserCString::from_syscall_arguments(valid_start, 4);
    let zero_pointer_rejected = UserReadableRange::from_syscall_arguments(0, 1).is_none();
    let zero_length_rejected = UserWritableRange::from_syscall_arguments(valid_start, 0).is_none();

    readable_range.is_some_and(|range| range.as_range().start().as_u64() == valid_start)
        && writable_range.is_some_and(|range| range.as_range().byte_len() == 4)
        && string_range.is_some_and(|range| {
            range.as_readable_range().as_range().end_exclusive() == valid_start + 4
        })
        && zero_pointer_rejected
        && zero_length_rejected
}

/// Verify typed address-to-`usize` conversion helper contracts.
pub fn verify_checked_address_conversions() -> bool {
    let page_size = usize::try_from(PAGE_SIZE).expect("test page size must fit in usize");
    let physical = PhysAddr::new(PAGE_SIZE);
    let dma = DmaPhysicalAddress::new(physical);
    let storage = StorageDataAddress::new(dma);
    let virtual_address = VirtAddr::new(PAGE_SIZE);
    let kernel = KernelVirtualAddress::new(virtual_address);
    let frame_start =
        PhysicalFrameStart::new(physical).expect("test physical frame start must be valid");
    let user =
        UserVirtualAddress::new(virtual_address).expect("test user virtual address must be valid");
    let user_page = UserPageStart::new(user).expect("test user page start must be valid");

    physical.try_as_usize() == Ok(page_size)
        && dma.try_as_usize() == Ok(page_size)
        && storage.try_as_usize() == Ok(page_size)
        && virtual_address.try_as_usize() == Ok(page_size)
        && kernel.try_as_usize() == Ok(page_size)
        && frame_start.try_as_usize() == Ok(page_size)
        && user.try_as_usize() == Ok(page_size)
        && user_page.try_as_usize() == Ok(page_size)
        && over_usize_address_is_rejected()
}

fn over_usize_address_is_rejected() -> bool {
    let Some(address) = u64::try_from(usize::MAX)
        .ok()
        .and_then(|max_address| max_address.checked_add(1))
    else {
        return true;
    };

    PhysAddr::new(address).try_as_usize() == Err(AddressConversionError::DoesNotFitUsize)
        && VirtAddr::new(address).try_as_usize() == Err(AddressConversionError::DoesNotFitUsize)
}

/// Verify the typed physical frame count construction contract.
pub fn verify_typed_frame_count() -> bool {
    let valid_count = FrameCount::new(2);
    let zero_count = FrameCount::new(0);
    let overflowing_count = FrameCount::new((u64::MAX / PAGE_SIZE) + 1);

    valid_count.is_some_and(|frame_count| {
        frame_count.as_u64() == 2 && frame_count.byte_len() == 2 * PAGE_SIZE
    }) && zero_count.is_none()
        && overflowing_count.is_none()
}

/// Verify the typed virtual page count construction contract.
pub fn verify_typed_page_count() -> bool {
    let valid_count = PageCount::new(2);
    let zero_count = PageCount::new(0);
    let overflowing_count = PageCount::new((u64::MAX / PAGE_SIZE) + 1);

    valid_count.is_some_and(|page_count| {
        page_count.as_u64() == 2 && page_count.byte_len() == 2 * PAGE_SIZE
    }) && zero_count.is_none()
        && overflowing_count.is_none()
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

    /// Return the first user page touched by this range.
    pub const fn first_page_start(self) -> Option<UserPageStart> {
        self.start.align_down_to_page()
    }

    /// Return the last user page touched by this range.
    pub const fn last_page_start(self) -> Option<UserPageStart> {
        let Some(last_byte) = self.end_exclusive().checked_sub(1) else {
            return None;
        };
        let Some(last_address) = UserVirtualAddress::new(VirtAddr::new(last_byte)) else {
            return None;
        };
        last_address.align_down_to_page()
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

    /// Convert raw syscall ABI pointer and length arguments into a readable range.
    pub fn from_syscall_arguments(user_pointer: u64, byte_len: u64) -> Option<Self> {
        let range = UserVirtualRange::from_syscall_arguments(user_pointer, byte_len)?;
        Some(Self::new(range))
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

    /// Convert raw syscall ABI pointer and length arguments into a C-string candidate.
    pub fn from_syscall_arguments(user_pointer: u64, byte_len: u64) -> Option<Self> {
        let range = UserReadableRange::from_syscall_arguments(user_pointer, byte_len)?;
        Some(Self::new(range))
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

    /// Convert raw syscall ABI pointer and length arguments into a writable range.
    pub fn from_syscall_arguments(user_pointer: u64, byte_len: u64) -> Option<Self> {
        let range = UserVirtualRange::from_syscall_arguments(user_pointer, byte_len)?;
        Some(Self::new(range))
    }

    /// Return the underlying user virtual range.
    pub const fn as_range(self) -> UserVirtualRange {
        self.0
    }
}
