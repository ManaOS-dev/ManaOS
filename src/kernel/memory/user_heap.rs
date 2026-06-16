//! User heap break tracking and page mapping.

use super::{
    address::{UserPageStart, UserVirtualAddress, VirtAddr},
    address_space::UserAddressSpace,
    frame_allocator::PhysicalFrameAllocator,
    user_layout::USER_HEAP_END,
};
use crate::kernel::memory::frame_allocator::FrameRangeOwner;
use x86_64::structures::paging::PageTableFlags;

const PAGE_SIZE: u64 = 4096;
const PAGE_SIZE_USIZE: usize = 4096;

/// Per-user-task heap break and mapped extent.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UserHeap {
    base: UserVirtualAddress,
    current_break: UserVirtualAddress,
    mapped_end: UserVirtualAddress,
}

impl UserHeap {
    /// Create a heap whose initial break starts after loaded user segments.
    ///
    /// # Panics
    ///
    /// Panics if the initial break is not page-aligned.
    pub fn new(initial_break: UserVirtualAddress) -> Self {
        assert!(
            initial_break.as_u64().is_multiple_of(PAGE_SIZE),
            "user heap initial break must be page-aligned"
        );
        assert!(
            initial_break.as_u64() < USER_HEAP_END,
            "user heap initial break must stay below the heap ceiling"
        );

        Self {
            base: initial_break,
            current_break: initial_break,
            mapped_end: initial_break,
        }
    }

    /// Return the heap base address.
    pub const fn base(self) -> UserVirtualAddress {
        self.base
    }

    /// Return the current heap break.
    pub const fn current_break(self) -> UserVirtualAddress {
        self.current_break
    }

    /// Return the first unmapped page after heap-backed mappings.
    pub const fn mapped_end(self) -> UserVirtualAddress {
        self.mapped_end
    }

    /// Return the number of pages currently mapped for this heap.
    pub const fn mapped_pages(self) -> u64 {
        (self.mapped_end.as_u64() - self.base.as_u64()) / PAGE_SIZE
    }

    /// Process a Linux-like `brk` request for this heap.
    ///
    /// A request of zero returns the current break. Invalid or out-of-memory
    /// growth requests leave the break unchanged and return the current break.
    /// Shrink requests unmap heap pages that are no longer covered by the
    /// requested break.
    pub fn process_break(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        requested_break: u64,
    ) -> UserVirtualAddress {
        if requested_break == 0 {
            return self.current_break;
        }
        if requested_break < self.base.as_u64() || requested_break >= USER_HEAP_END {
            return self.current_break;
        }

        let Some(requested_break) = UserVirtualAddress::new(VirtAddr::new(requested_break)) else {
            return self.current_break;
        };
        let mapped_end = align_up_to_page(requested_break.as_u64());
        if mapped_end > self.mapped_end.as_u64()
            && !self.map_new_pages(address_space, frame_allocator, mapped_end)
        {
            return self.current_break;
        }
        if mapped_end < self.mapped_end.as_u64() {
            self.unmap_old_pages(address_space, frame_allocator, mapped_end);
        }

        self.current_break = requested_break;
        self.current_break
    }

    fn map_new_pages(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        mapped_end: u64,
    ) -> bool {
        let mut page_start =
            UserPageStart::new(self.mapped_end).expect("user heap mapped end must be page-aligned");
        while page_start.as_u64() < mapped_end {
            let Some(physical_address) =
                frame_allocator.allocate_frame_for(FrameRangeOwner::UserHeap)
            else {
                return false;
            };
            let page_pointer = physical_address.as_usize() as *mut u8;
            // SAFETY: `physical_address` is a freshly allocated identity-mapped
            // user heap frame.
            unsafe {
                core::ptr::write_bytes(page_pointer, 0, PAGE_SIZE_USIZE);
            }
            address_space.map_user_page(
                frame_allocator,
                page_start,
                physical_address,
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE
                    | PageTableFlags::NO_EXECUTE,
            );
            page_start = page_start
                .checked_add(PAGE_SIZE)
                .expect("user heap mapping address overflowed");
            self.mapped_end = page_start.as_address();
        }
        true
    }

    fn unmap_old_pages(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        mapped_end: u64,
    ) {
        let mut page_start =
            UserPageStart::new(self.mapped_end).expect("user heap mapped end must be page-aligned");
        let mut unmapped_pages = 0_u64;
        while page_start.as_u64() > mapped_end {
            page_start = page_start
                .checked_sub(PAGE_SIZE)
                .expect("user heap unmapping address underflowed");
            assert!(
                address_space.unmap_user_page_for(
                    frame_allocator,
                    page_start,
                    FrameRangeOwner::UserHeap,
                ),
                "shrinking user heap must unmap a mapped heap page"
            );
            self.mapped_end = page_start.as_address();
            unmapped_pages = unmapped_pages.saturating_add(1);
        }
        crate::log_info!(
            "memory",
            "User heap pages unmapped: new_mapped_end={:#x} pages={}",
            self.mapped_end.as_u64(),
            unmapped_pages
        );
    }
}

fn align_up_to_page(address: u64) -> u64 {
    (address + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}
