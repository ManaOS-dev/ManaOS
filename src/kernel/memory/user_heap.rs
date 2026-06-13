//! User heap break tracking and page mapping.

use super::{
    address::UserVirtualAddress, address_space::UserAddressSpace,
    frame_allocator::PhysicalFrameAllocator,
};
use crate::kernel::memory::frame_allocator::FrameRangeOwner;
use x86_64::structures::paging::PageTableFlags;

const PAGE_SIZE: u64 = 4096;
const PAGE_SIZE_USIZE: usize = 4096;
// Keep the first heap model well below the fixed stack slots while process
// address layout is still static.
const USER_HEAP_MAX_END: u64 = 0x0000_7000_0000_0000;

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
            initial_break.as_u64() < USER_HEAP_MAX_END,
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
    pub fn process_break(
        &mut self,
        address_space: UserAddressSpace,
        frame_allocator: &mut PhysicalFrameAllocator,
        requested_break: u64,
    ) -> UserVirtualAddress {
        if requested_break == 0 {
            return self.current_break;
        }
        if requested_break < self.base.as_u64() || requested_break >= USER_HEAP_MAX_END {
            return self.current_break;
        }

        let Some(requested_break) = UserVirtualAddress::new(requested_break) else {
            return self.current_break;
        };
        let mapped_end = align_up_to_page(requested_break.as_u64());
        if mapped_end > self.mapped_end.as_u64()
            && !self.map_new_pages(address_space, frame_allocator, mapped_end)
        {
            return self.current_break;
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
        let mut page_start = self.mapped_end;
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
            self.mapped_end = page_start;
        }
        true
    }
}

fn align_up_to_page(address: u64) -> u64 {
    (address + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}
