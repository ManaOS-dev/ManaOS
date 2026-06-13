//! Anonymous user mapping tracking and page mapping.

use super::{
    address::UserVirtualAddress,
    address_space::UserAddressSpace,
    frame_allocator::{FrameRangeOwner, PhysicalFrameAllocator},
    user_layout::{USER_MAPPING_BASE, USER_MAPPING_END},
};
use x86_64::structures::paging::PageTableFlags;

const PAGE_SIZE: u64 = 4096;
const PAGE_SIZE_USIZE: usize = 4096;
const MAX_USER_MAPPINGS: usize = 32;

/// Result of one anonymous user mapping allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMappingAllocation {
    start: UserVirtualAddress,
    page_count: u64,
}

impl UserMappingAllocation {
    /// Return the first virtual address in the mapping.
    pub const fn start(self) -> UserVirtualAddress {
        self.start
    }

    /// Return the number of mapped 4 KiB pages.
    pub const fn page_count(self) -> u64 {
        self.page_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UserMapping {
    start: UserVirtualAddress,
    page_count: u64,
}

impl UserMapping {
    fn end_exclusive(self) -> Option<u64> {
        self.start
            .as_u64()
            .checked_add(self.page_count.checked_mul(PAGE_SIZE)?)
    }
}

/// Anonymous user mappings owned by one user task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMappings {
    next_start: u64,
    records: [Option<UserMapping>; MAX_USER_MAPPINGS],
}

impl UserMappings {
    /// Create an empty anonymous mapping table.
    pub const fn new() -> Self {
        Self {
            next_start: USER_MAPPING_BASE,
            records: [None; MAX_USER_MAPPINGS],
        }
    }

    /// Map one anonymous private user range.
    ///
    /// Returns `None` when the request is outside the anonymous mapping region,
    /// the fixed record table is full, or a backing frame cannot be allocated.
    pub fn map_anonymous(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        length: u64,
        writable: bool,
    ) -> Option<UserMappingAllocation> {
        let page_count = page_count_for_length(length)?;
        let record_index = self.next_empty_record_index()?;
        let byte_len = page_count.checked_mul(PAGE_SIZE)?;
        let start_address = self.next_start;
        let end_address = start_address.checked_add(byte_len)?;
        if start_address < USER_MAPPING_BASE || end_address > USER_MAPPING_END {
            return None;
        }

        let start = UserVirtualAddress::new(start_address)?;
        if !Self::map_pages(address_space, frame_allocator, start, page_count, writable) {
            return None;
        }

        self.records[record_index] = Some(UserMapping { start, page_count });
        self.next_start = end_address;
        Some(UserMappingAllocation { start, page_count })
    }

    /// Unmap a page-aligned anonymous mapping range and return removed pages.
    ///
    /// The range must be fully contained in one existing mapping record. When
    /// the removed range is in the middle of a record, the record is split so
    /// both remaining sides stay tracked.
    pub fn unmap_range(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        start_address: u64,
        length: u64,
    ) -> Option<u64> {
        if !start_address.is_multiple_of(PAGE_SIZE) {
            return None;
        }
        let start = UserVirtualAddress::new(start_address)?;
        let page_count = page_count_for_length(length)?;
        let end_address = start_address.checked_add(page_count.checked_mul(PAGE_SIZE)?)?;
        let record_index = self.find_containing_record_index(start_address, end_address)?;
        let record = self.records[record_index].expect("containing record must exist");
        let record_start = record.start.as_u64();
        let record_end = record
            .end_exclusive()
            .expect("tracked mapping end must not overflow");
        let left_pages = (start_address - record_start) / PAGE_SIZE;
        let right_pages = (record_end - end_address) / PAGE_SIZE;
        let split_record_index = if left_pages > 0 && right_pages > 0 {
            Some(self.next_empty_record_index()?)
        } else {
            None
        };

        Self::unmap_pages(address_space, frame_allocator, start, page_count);
        self.apply_record_unmap(
            record_index,
            split_record_index,
            record,
            left_pages,
            right_pages,
            end_address,
        );
        crate::log_info!(
            "memory",
            "User anonymous mapping unmapped: start={:#x} pages={} records={} active_pages={}",
            start.as_u64(),
            page_count,
            self.active_records(),
            self.active_pages()
        );
        Some(page_count)
    }

    /// Return currently mapped anonymous user pages.
    pub fn active_pages(&self) -> u64 {
        self.records
            .iter()
            .filter_map(|record| record.as_ref().map(|mapping| mapping.page_count))
            .fold(0_u64, u64::saturating_add)
    }

    /// Return the number of active anonymous mapping records.
    pub fn active_records(&self) -> u64 {
        self.records
            .iter()
            .filter(|record| record.is_some())
            .count()
            .try_into()
            .expect("active mapping record count must fit in u64")
    }

    /// Return the next anonymous mapping search start.
    pub const fn next_start(&self) -> u64 {
        self.next_start
    }

    fn map_pages(
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        start: UserVirtualAddress,
        page_count: u64,
        writable: bool,
    ) -> bool {
        let flags = user_page_flags(writable);
        let mut mapped_pages = 0_u64;
        while mapped_pages < page_count {
            let Some(page_start) = user_page_start(start, mapped_pages) else {
                Self::unmap_prefix(address_space, frame_allocator, start, mapped_pages);
                return false;
            };
            let Some(physical_address) =
                frame_allocator.allocate_frame_for(FrameRangeOwner::UserMapping)
            else {
                Self::unmap_prefix(address_space, frame_allocator, start, mapped_pages);
                return false;
            };

            let page_pointer = physical_address.as_usize() as *mut u8;
            // SAFETY: `physical_address` is a freshly allocated identity-mapped
            // anonymous user mapping frame.
            unsafe {
                core::ptr::write_bytes(page_pointer, 0, PAGE_SIZE_USIZE);
            }
            address_space.map_user_page(frame_allocator, page_start, physical_address, flags);
            mapped_pages = mapped_pages.saturating_add(1);
        }
        true
    }

    fn unmap_pages(
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        start: UserVirtualAddress,
        page_count: u64,
    ) {
        for page_index in 0..page_count {
            let page_start =
                user_page_start(start, page_index).expect("tracked mapping page must be valid");
            assert!(
                address_space.unmap_user_page_for(
                    frame_allocator,
                    page_start,
                    FrameRangeOwner::UserMapping,
                ),
                "tracked anonymous user mapping page must be mapped"
            );
        }
    }

    fn unmap_prefix(
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        start: UserVirtualAddress,
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
                "mapped anonymous prefix page must be mapped"
            );
        }
    }

    fn next_empty_record_index(self) -> Option<usize> {
        self.records.iter().position(Option::is_none)
    }

    fn find_containing_record_index(self, start_address: u64, end_address: u64) -> Option<usize> {
        self.records.iter().position(|record| {
            let Some(mapping) = record else {
                return false;
            };
            let Some(mapping_end) = mapping.end_exclusive() else {
                return false;
            };
            mapping.start.as_u64() <= start_address && end_address <= mapping_end
        })
    }

    fn apply_record_unmap(
        &mut self,
        record_index: usize,
        split_record_index: Option<usize>,
        record: UserMapping,
        left_pages: u64,
        right_pages: u64,
        right_start_address: u64,
    ) {
        match (left_pages, right_pages) {
            (0, 0) => self.records[record_index] = None,
            (_, 0) => {
                self.records[record_index] = Some(UserMapping {
                    start: record.start,
                    page_count: left_pages,
                });
            }
            (0, _) => {
                let right_start = UserVirtualAddress::new(right_start_address)
                    .expect("right split mapping start must be a valid user address");
                self.records[record_index] = Some(UserMapping {
                    start: right_start,
                    page_count: right_pages,
                });
            }
            (_, _) => {
                let split_record_index =
                    split_record_index.expect("middle unmap must reserve a split record");
                let right_start = UserVirtualAddress::new(right_start_address)
                    .expect("right split mapping start must be a valid user address");
                self.records[record_index] = Some(UserMapping {
                    start: record.start,
                    page_count: left_pages,
                });
                self.records[split_record_index] = Some(UserMapping {
                    start: right_start,
                    page_count: right_pages,
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

fn page_count_for_length(length: u64) -> Option<u64> {
    if length == 0 {
        return None;
    }
    let rounded_length = length.checked_add(PAGE_SIZE - 1)? & !(PAGE_SIZE - 1);
    Some(rounded_length / PAGE_SIZE)
}

fn user_page_start(start: UserVirtualAddress, page_index: u64) -> Option<UserVirtualAddress> {
    let offset = page_index.checked_mul(PAGE_SIZE)?;
    start.checked_add(offset)
}
