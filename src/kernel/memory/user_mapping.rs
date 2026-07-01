//! User private mapping tracking and page mapping.

use super::{
    address::{
        FrameCount, PageCount, PhysicalFrameRange, UserPageStart, UserVirtualAddress, VirtAddr,
    },
    address_space::UserAddressSpace,
    frame_allocator::{FrameRangeOwner, PhysicalFrameAllocator},
    user_layout::{USER_MAPPING_BASE, USER_MAPPING_END},
};
use x86_64::structures::paging::PageTableFlags;

const PAGE_SIZE: u64 = 4096;
const PAGE_SIZE_USIZE: usize = 4096;
const MAX_USER_MAPPINGS: usize = 32;

/// Result of one private user mapping allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMappingAllocation {
    start: UserVirtualAddress,
    page_count: PageCount,
    replaced_page_count: u64,
}

impl UserMappingAllocation {
    /// Return the first virtual address in the mapping.
    pub const fn start(self) -> UserVirtualAddress {
        self.start
    }

    /// Return the typed count of mapped 4 KiB pages.
    pub const fn page_count(self) -> PageCount {
        self.page_count
    }

    /// Return pages released while replacing overlapping mappings.
    pub const fn replaced_page_count(self) -> u64 {
        self.replaced_page_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UserMapping {
    start: UserPageStart,
    page_count: PageCount,
    source: UserMappingSource,
}

impl UserMapping {
    fn range(self) -> Option<UserMappingRange> {
        UserMappingRange::new(self.start, self.page_count)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UserMappingRange {
    start: UserPageStart,
    end_exclusive: UserPageStart,
    page_count: PageCount,
}

impl UserMappingRange {
    fn new(start: UserPageStart, page_count: PageCount) -> Option<Self> {
        let end_exclusive = start.checked_add(page_count.byte_len())?;
        if start.as_u64() < USER_MAPPING_BASE || end_exclusive.as_u64() > USER_MAPPING_END {
            return None;
        }
        Some(Self {
            start,
            end_exclusive,
            page_count,
        })
    }

    fn from_byte_len(start: UserPageStart, byte_len: u64) -> Option<Self> {
        if byte_len == 0 || !byte_len.is_multiple_of(PAGE_SIZE) {
            return None;
        }
        Self::new(start, PageCount::new(byte_len / PAGE_SIZE)?)
    }

    const fn start(self) -> UserPageStart {
        self.start
    }

    const fn page_count(self) -> PageCount {
        self.page_count
    }

    const fn end_exclusive(self) -> UserPageStart {
        self.end_exclusive
    }

    const fn start_address(self) -> u64 {
        self.start.as_u64()
    }

    const fn end_address(self) -> u64 {
        self.end_exclusive.as_u64()
    }

    const fn overlaps(self, other: Self) -> bool {
        self.start_address() < other.end_address() && other.start_address() < self.end_address()
    }

    const fn contains(self, other: Self) -> bool {
        self.start_address() <= other.start_address() && other.end_address() <= self.end_address()
    }
}

/// Source used to initialize a private user mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserMappingSource {
    /// Mapping pages start as zero-filled anonymous memory.
    Anonymous,
    /// Mapping pages start as a private copy of file bytes.
    FilePrivate,
}

impl UserMappingSource {
    /// Return a stable diagnostic label for this mapping source.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anonymous => "anonymous",
            Self::FilePrivate => "file_private",
        }
    }
}

/// Non-empty private mapping syscall length with its rounded page count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMappingLength {
    byte_len: u64,
    page_count: PageCount,
}

impl UserMappingLength {
    /// Convert a raw syscall mapping length into a typed mapping length.
    pub fn from_syscall_argument(byte_len: u64) -> Option<Self> {
        let page_count = page_count_for_length(byte_len)?;
        Some(Self {
            byte_len,
            page_count,
        })
    }

    /// Return the raw requested byte length for diagnostics and page preload.
    pub const fn byte_len(self) -> u64 {
        self.byte_len
    }

    /// Return the rounded 4 KiB page count covered by this length.
    pub const fn page_count(self) -> PageCount {
        self.page_count
    }
}

/// Mapping parameters shared by anonymous and file-private mappings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMappingPlan {
    placement: UserMappingPlacement,
    length: UserMappingLength,
    writable: bool,
    source: UserMappingSource,
}

impl UserMappingPlan {
    /// Create mapping parameters for one private user mapping.
    pub const fn new(
        placement: UserMappingPlacement,
        length: UserMappingLength,
        writable: bool,
        source: UserMappingSource,
    ) -> Self {
        Self {
            placement,
            length,
            writable,
            source,
        }
    }

    /// Return the placement policy for the mapping.
    pub const fn placement(self) -> UserMappingPlacement {
        self.placement
    }

    /// Return the requested mapping length in bytes.
    pub const fn length(self) -> u64 {
        self.length.byte_len()
    }

    /// Return the rounded page count for the requested mapping length.
    pub const fn page_count(self) -> PageCount {
        self.length.page_count()
    }

    /// Return whether user code may write the mapped pages.
    pub const fn writable(self) -> bool {
        self.writable
    }

    /// Return the data source used to initialize the mapping.
    pub const fn source(self) -> UserMappingSource {
        self.source
    }
}

/// A private user unmapping request after syscall ABI address classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMappingUnmapRequest {
    start: UserPageStart,
    length: UserMappingLength,
}

impl UserMappingUnmapRequest {
    /// Convert raw syscall `munmap` arguments into a private unmapping request.
    pub fn from_syscall_arguments(start_address: u64, length: u64) -> Option<Self> {
        let start = user_page_start_from_raw(start_address)?;
        let length = UserMappingLength::from_syscall_argument(length)?;

        Some(Self { start, length })
    }

    /// Return the first page in the unmapping request.
    pub const fn start(self) -> UserPageStart {
        self.start
    }

    /// Return the raw requested byte length for diagnostics.
    pub const fn length(self) -> u64 {
        self.length.byte_len()
    }

    /// Return the rounded 4 KiB page count covered by this request.
    pub const fn page_count(self) -> PageCount {
        self.length.page_count()
    }
}

/// Placement policy for a private user mapping request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserMappingPlacement {
    /// Choose the next available address in the user mapping region.
    Any,
    /// Use the requested address only when the range is currently unmapped.
    FixedNoReplace(UserPageStart),
    /// Use the requested address after replacing overlapping private mappings.
    FixedReplace(UserPageStart),
}

impl UserMappingPlacement {
    /// Return a stable diagnostic label for this placement policy.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::FixedNoReplace(_) => "fixed_noreplace",
            Self::FixedReplace(_) => "fixed_replace",
        }
    }
}

/// Reason a private mapping request was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserMappingError {
    /// The request could not fit the user mapping region.
    InvalidRequest,
    /// The requested fixed range overlaps an active mapping.
    AddressInUse,
    /// The request ran out of physical frames or mapping records.
    OutOfMemory,
    /// Page initialization failed after the virtual range was reserved.
    InitializationFailed,
}

/// Private user mappings owned by one user task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMappings {
    next_start: UserPageStart,
    records: [Option<UserMapping>; MAX_USER_MAPPINGS],
}

impl UserMappings {
    /// Create an empty private mapping table.
    pub const fn new() -> Self {
        Self {
            next_start: UserPageStart::new(
                UserVirtualAddress::new(VirtAddr::new(USER_MAPPING_BASE))
                    .expect("user mapping base must be a valid user virtual address"),
            )
            .expect("user mapping base must be page-aligned"),
            records: [None; MAX_USER_MAPPINGS],
        }
    }

    /// Map one private user range.
    ///
    /// Returns an error when the request is outside the user mapping region,
    /// the fixed record table is full, a backing frame cannot be allocated, or
    /// the page initializer rejects a page.
    pub fn map_private(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        plan: UserMappingPlan,
        mut initialize_page: impl FnMut(u64, &mut [u8]) -> Result<(), UserMappingError>,
    ) -> Result<UserMappingAllocation, UserMappingError> {
        let length = plan.length();
        let page_count = plan.page_count();
        let byte_len = page_count.byte_len();
        let start = self.start_address_for_placement(plan.placement(), byte_len)?;
        let requested_range =
            UserMappingRange::new(start, page_count).ok_or(UserMappingError::InvalidRequest)?;
        let start_address = requested_range.start_address();
        let mut replaced_page_count = 0_u64;
        if matches!(plan.placement(), UserMappingPlacement::FixedReplace(_)) {
            self.ensure_replace_record_capacity(requested_range)?;
            replaced_page_count =
                self.replace_overlapping_pages(address_space, frame_allocator, requested_range);
            if replaced_page_count > 0 {
                crate::log_info!(
                    "memory",
                    "User mapping fixed replacement prepared: start={:#x} pages={} records={} active_pages={} mapping_range_typed=true mapping_range_end_typed=true",
                    start_address,
                    replaced_page_count,
                    self.active_records(),
                    self.active_pages()
                );
            }
        }
        let record_index = self
            .next_empty_record_index()
            .ok_or(UserMappingError::OutOfMemory)?;

        Self::map_pages(
            address_space,
            frame_allocator,
            start,
            length,
            page_count,
            plan.writable(),
            &mut initialize_page,
        )?;

        self.records[record_index] = Some(UserMapping {
            start,
            page_count,
            source: plan.source(),
        });
        if matches!(plan.placement(), UserMappingPlacement::Any) {
            self.next_start = requested_range.end_exclusive();
        }
        Ok(UserMappingAllocation {
            start: requested_range.start().as_address(),
            page_count: requested_range.page_count(),
            replaced_page_count,
        })
    }

    /// Unmap a page-aligned private mapping range and return removed pages.
    ///
    /// The range must be fully contained in one existing mapping record. When
    /// the removed range is in the middle of a record, the record is split so
    /// both remaining sides stay tracked.
    pub fn unmap_range(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        request: UserMappingUnmapRequest,
    ) -> Option<PageCount> {
        let start = request.start();
        let page_count = request.page_count();
        let requested_range = UserMappingRange::new(start, page_count)?;
        let start_address = requested_range.start_address();
        let end_address = requested_range.end_address();
        let record_index = self.find_containing_record_index(requested_range)?;
        let record = self.records[record_index].expect("containing record must exist");
        let record_range = record
            .range()
            .expect("containing record range must be valid");
        let record_start = record_range.start_address();
        let record_end = record_range.end_address();
        let left_pages = (start_address - record_start) / PAGE_SIZE;
        let right_pages = (record_end - end_address) / PAGE_SIZE;
        let split_record_index = if left_pages > 0 && right_pages > 0 {
            Some(self.next_empty_record_index()?)
        } else {
            None
        };

        let source = record.source;
        Self::unmap_pages(address_space, frame_allocator, start, page_count);
        let right_start = requested_range.end_exclusive();
        self.apply_record_unmap(
            record_index,
            split_record_index,
            record,
            left_pages,
            right_pages,
            right_start,
        );
        crate::log_info!(
            "memory",
            "User {} mapping unmapped: start={:#x} pages={} records={} active_pages={} page_count_typed=true split_start_typed=true record_start_typed=true unmap_range_typed=true mapping_range_end_typed=true",
            source.as_str(),
            requested_range.start_address(),
            page_count.as_u64(),
            self.active_records(),
            self.active_pages()
        );
        Some(page_count)
    }

    /// Return currently mapped private user pages.
    pub fn active_pages(&self) -> u64 {
        self.records
            .iter()
            .filter_map(|record| record.as_ref().map(|mapping| mapping.page_count.as_u64()))
            .fold(0_u64, u64::saturating_add)
    }

    /// Return the number of active private mapping records.
    pub fn active_records(&self) -> u64 {
        self.records
            .iter()
            .filter(|record| record.is_some())
            .count()
            .try_into()
            .expect("active mapping record count must fit in u64")
    }

    /// Return the number of active file-private mapping records.
    pub fn active_file_private_records(&self) -> u64 {
        self.records
            .iter()
            .filter(|record| {
                record
                    .as_ref()
                    .is_some_and(|mapping| mapping.source == UserMappingSource::FilePrivate)
            })
            .count()
            .try_into()
            .expect("active file mapping record count must fit in u64")
    }

    /// Return the next mapping search start.
    pub const fn next_start(&self) -> UserVirtualAddress {
        self.next_start.as_address()
    }

    fn map_pages(
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        start: UserPageStart,
        length: u64,
        page_count: PageCount,
        writable: bool,
        initialize_page: &mut impl FnMut(u64, &mut [u8]) -> Result<(), UserMappingError>,
    ) -> Result<(), UserMappingError> {
        let flags = user_page_flags(writable);
        let page_count = page_count.as_u64();
        let mut mapped_pages = 0_u64;
        while mapped_pages < page_count {
            let Some(page_start) = user_page_start(start, mapped_pages) else {
                Self::unmap_prefix(address_space, frame_allocator, start, mapped_pages);
                return Err(UserMappingError::InvalidRequest);
            };
            let Some(physical_address) =
                frame_allocator.allocate_frame_for(FrameRangeOwner::UserMapping)
            else {
                Self::unmap_prefix(address_space, frame_allocator, start, mapped_pages);
                return Err(UserMappingError::OutOfMemory);
            };

            let page_pointer = physical_address.as_usize() as *mut u8;
            // SAFETY: `physical_address` is a freshly allocated identity-mapped
            // private user mapping frame.
            unsafe {
                core::ptr::write_bytes(page_pointer, 0, PAGE_SIZE_USIZE);
            }
            let page_length = page_initialize_length(length, mapped_pages)
                .ok_or(UserMappingError::InvalidRequest)?;
            if page_length > 0 {
                // SAFETY: `physical_address` is a freshly allocated
                // identity-mapped frame, zeroed above, and not visible to user
                // mode until after `initialize_page` returns.
                let page_buffer =
                    unsafe { core::slice::from_raw_parts_mut(page_pointer, PAGE_SIZE_USIZE) };
                if let Err(error) = initialize_page(mapped_pages, &mut page_buffer[..page_length]) {
                    Self::free_unmapped_page(frame_allocator, physical_address);
                    Self::unmap_prefix(address_space, frame_allocator, start, mapped_pages);
                    return Err(error);
                }
            }
            address_space.map_user_page(frame_allocator, page_start, physical_address, flags);
            mapped_pages = mapped_pages.saturating_add(1);
        }
        Ok(())
    }

    fn unmap_pages(
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        start: UserPageStart,
        page_count: PageCount,
    ) {
        for page_index in 0..page_count.as_u64() {
            let page_start =
                user_page_start(start, page_index).expect("tracked mapping page must be valid");
            assert!(
                address_space.unmap_user_page_for(
                    frame_allocator,
                    page_start,
                    FrameRangeOwner::UserMapping,
                ),
                "tracked private user mapping page must be mapped"
            );
        }
    }

    fn unmap_prefix(
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        start: UserPageStart,
        page_count: u64,
    ) {
        for page_index in 0..page_count {
            let page_start =
                user_page_start(start, page_index).expect("mapped prefix page must be valid");
            assert!(
                address_space.unmap_user_page_for(
                    frame_allocator,
                    page_start,
                    FrameRangeOwner::UserMapping,
                ),
                "mapped private prefix page must be mapped"
            );
        }
    }

    fn free_unmapped_page(
        frame_allocator: &mut PhysicalFrameAllocator,
        physical_address: super::address::PhysicalFrameStart,
    ) {
        let physical_range = PhysicalFrameRange::new(physical_address, single_frame_count())
            .expect("single user mapping frame range must be valid");
        assert!(
            frame_allocator.free_frames_for(physical_range, FrameRangeOwner::UserMapping),
            "unmapped private user mapping page must be owned by user mappings"
        );
    }

    fn next_empty_record_index(self) -> Option<usize> {
        self.records.iter().position(Option::is_none)
    }

    fn start_address_for_placement(
        self,
        placement: UserMappingPlacement,
        byte_len: u64,
    ) -> Result<UserPageStart, UserMappingError> {
        match placement {
            UserMappingPlacement::Any => self
                .next_available_start(self.next_start, byte_len)
                .ok_or(UserMappingError::OutOfMemory),
            UserMappingPlacement::FixedReplace(start) => fixed_start_address(start, byte_len),
            UserMappingPlacement::FixedNoReplace(start) => {
                let start = fixed_start_address(start, byte_len)?;
                let requested_range = UserMappingRange::from_byte_len(start, byte_len)
                    .ok_or(UserMappingError::InvalidRequest)?;
                if self.overlapping_record_end(requested_range).is_some() {
                    return Err(UserMappingError::AddressInUse);
                }
                Ok(start)
            }
        }
    }

    fn ensure_replace_record_capacity(
        self,
        requested_range: UserMappingRange,
    ) -> Result<(), UserMappingError> {
        let mut record_count: usize = self
            .active_records()
            .try_into()
            .expect("active mapping record count must fit in usize");
        for mapping in self.records.iter().flatten() {
            let mapping_range = mapping.range().ok_or(UserMappingError::InvalidRequest)?;
            if !requested_range.overlaps(mapping_range) {
                continue;
            }

            record_count -= 1;
            if mapping_range.start_address() < requested_range.start_address() {
                record_count += 1;
            }
            if requested_range.end_address() < mapping_range.end_address() {
                record_count += 1;
            }
        }

        if record_count < MAX_USER_MAPPINGS {
            Ok(())
        } else {
            Err(UserMappingError::OutOfMemory)
        }
    }

    fn replace_overlapping_pages(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        requested_range: UserMappingRange,
    ) -> u64 {
        let mut replaced_pages = 0_u64;
        for record_index in 0..self.records.len() {
            let Some(record) = self.records[record_index] else {
                continue;
            };
            let record_range = record.range().expect("tracked mapping range must be valid");
            if !requested_range.overlaps(record_range) {
                continue;
            }

            let record_start = record_range.start_address();
            let record_end = record_range.end_address();
            let overlap_start_address = record_start.max(requested_range.start_address());
            let overlap_end_address = record_end.min(requested_range.end_address());
            let overlap_pages = (overlap_end_address - overlap_start_address) / PAGE_SIZE;
            let overlap_start = user_page_start_from_raw(overlap_start_address)
                .expect("replacement overlap start must be a valid user address");
            let overlap_pages = page_count(overlap_pages);
            let overlap_range = UserMappingRange::new(overlap_start, overlap_pages)
                .expect("replacement overlap range must be valid");
            Self::unmap_pages(
                address_space,
                frame_allocator,
                overlap_range.start(),
                overlap_range.page_count(),
            );
            replaced_pages = replaced_pages.saturating_add(overlap_range.page_count().as_u64());

            let left_pages = (overlap_range.start_address() - record_start) / PAGE_SIZE;
            let right_pages = (record_end - overlap_range.end_address()) / PAGE_SIZE;
            let right_start = overlap_range.end_exclusive();
            let split_record_index = if left_pages > 0 && right_pages > 0 {
                Some(
                    self.next_empty_record_index()
                        .expect("replacement preflight must reserve split records"),
                )
            } else {
                None
            };
            self.apply_record_unmap(
                record_index,
                split_record_index,
                record,
                left_pages,
                right_pages,
                right_start,
            );
        }
        replaced_pages
    }

    fn next_available_start(
        self,
        preferred_start: UserPageStart,
        byte_len: u64,
    ) -> Option<UserPageStart> {
        let mut candidate = preferred_start;
        loop {
            let candidate_range = UserMappingRange::from_byte_len(candidate, byte_len)?;
            let Some(overlap_end) = self.overlapping_record_end(candidate_range) else {
                return Some(candidate);
            };
            candidate = overlap_end;
        }
    }

    fn overlapping_record_end(self, requested_range: UserMappingRange) -> Option<UserPageStart> {
        self.records
            .iter()
            .filter_map(|record| {
                let mapping = record.as_ref()?;
                let mapping_range = mapping.range()?;
                if requested_range.overlaps(mapping_range) {
                    Some(mapping_range.end_exclusive())
                } else {
                    None
                }
            })
            .max_by_key(|page_start| page_start.as_u64())
    }

    fn find_containing_record_index(self, requested_range: UserMappingRange) -> Option<usize> {
        self.records.iter().position(|record| {
            let Some(mapping) = record else {
                return false;
            };
            let Some(mapping_range) = mapping.range() else {
                return false;
            };
            mapping_range.contains(requested_range)
        })
    }

    fn apply_record_unmap(
        &mut self,
        record_index: usize,
        split_record_index: Option<usize>,
        record: UserMapping,
        left_pages: u64,
        right_pages: u64,
        right_start: UserPageStart,
    ) {
        match (left_pages, right_pages) {
            (0, 0) => self.records[record_index] = None,
            (_, 0) => {
                self.records[record_index] = Some(UserMapping {
                    start: record.start,
                    page_count: page_count(left_pages),
                    source: record.source,
                });
            }
            (0, _) => {
                self.records[record_index] = Some(UserMapping {
                    start: right_start,
                    page_count: page_count(right_pages),
                    source: record.source,
                });
            }
            (_, _) => {
                let split_record_index =
                    split_record_index.expect("middle unmap must reserve a split record");
                self.records[record_index] = Some(UserMapping {
                    start: record.start,
                    page_count: page_count(left_pages),
                    source: record.source,
                });
                self.records[split_record_index] = Some(UserMapping {
                    start: right_start,
                    page_count: page_count(right_pages),
                    source: record.source,
                });
            }
        }
    }
}

fn user_page_flags(writable: bool) -> PageTableFlags {
    let mut flags =
        PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE;
    if writable {
        flags |= PageTableFlags::WRITABLE;
    }
    flags
}

fn page_count_for_length(length: u64) -> Option<PageCount> {
    if length == 0 {
        return None;
    }
    let rounded_length = length.checked_add(PAGE_SIZE - 1)? & !(PAGE_SIZE - 1);
    PageCount::new(rounded_length / PAGE_SIZE)
}

fn page_initialize_length(length: u64, page_index: u64) -> Option<usize> {
    let page_start_offset = page_index.checked_mul(PAGE_SIZE)?;
    let remaining = length.saturating_sub(page_start_offset);
    usize::try_from(remaining.min(PAGE_SIZE)).ok()
}

fn fixed_start_address(
    start: UserPageStart,
    byte_len: u64,
) -> Result<UserPageStart, UserMappingError> {
    let start_address = start.as_u64();
    let end_address = start_address
        .checked_add(byte_len)
        .ok_or(UserMappingError::InvalidRequest)?;
    if start_address < USER_MAPPING_BASE || end_address > USER_MAPPING_END {
        return Err(UserMappingError::InvalidRequest);
    }
    Ok(start)
}

fn user_page_start_from_raw(address: u64) -> Option<UserPageStart> {
    let address = UserVirtualAddress::new(VirtAddr::new(address))?;
    UserPageStart::new(address)
}

const fn single_frame_count() -> FrameCount {
    FrameCount::new(1).expect("single-frame count must be valid")
}

fn page_count(count: u64) -> PageCount {
    PageCount::new(count).expect("user mapping page count must be valid")
}

fn user_page_start(start: UserPageStart, page_index: u64) -> Option<UserPageStart> {
    let offset = page_index.checked_mul(PAGE_SIZE)?;
    start.checked_add(offset)
}
