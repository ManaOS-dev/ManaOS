//! User-space stack mapping.

use crate::kernel::memory::frame_allocator::BumpFrameAllocator;
use x86_64::{
    instructions::tlb,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB,
    },
    PhysAddr, VirtAddr,
};

const PAGE_SIZE: u64 = 4096;
const USER_STACK_BASE: u64 = 0x0000_7fff_f000_0000;

/// Allocate and map a fixed-base user-space stack.
///
/// Returns the virtual address one byte past the mapped stack range.
///
/// # Panics
///
/// Panics if physical frames cannot be allocated or page-table mapping fails.
pub fn allocate_user_stack(frame_allocator: &mut BumpFrameAllocator, pages: u64) -> u64 {
    let physical_start = frame_allocator
        .allocate_frames(pages)
        .unwrap_or_else(|| panic!("OOM: failed to allocate {pages} pages for user stack"));

    // SAFETY: The active level-4 page table is identity mapped by early paging,
    // and the provided allocator supplies page-table frames for missing levels.
    unsafe {
        map_user_range(
            frame_allocator,
            USER_STACK_BASE,
            physical_start,
            pages,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
        );
    }

    USER_STACK_BASE + pages * PAGE_SIZE
}

/// Mark an already mapped page as accessible from user space.
///
/// This is a temporary bridge for the built-in user stub. The ELF loader will
/// replace it with dedicated user text mappings.
///
/// # Panics
///
/// Panics if `virtual_address` is not mapped in the active page table.
pub fn allow_user_access_to_existing_page(virtual_address: u64) {
    let virtual_address = VirtAddr::new(virtual_address);
    // SAFETY: The active page table is identity mapped and the requested page
    // must already be present because the entry point is kernel text.
    unsafe {
        mark_page_table_path_user_accessible(virtual_address);
    }
    tlb::flush(virtual_address);
}

unsafe fn map_user_range(
    frame_allocator: &mut BumpFrameAllocator,
    virtual_start: u64,
    physical_start: u64,
    pages: u64,
    flags: PageTableFlags,
) {
    let (level_4_frame, _) = Cr3::read();
    let level_4_table = level_4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: The active page table is identity mapped, so its physical address
    // is a valid virtual address in the current address space.
    let level_4_table = unsafe { &mut *level_4_table };
    // SAFETY: ManaOS uses identity-mapped physical memory for page-table access.
    let mut mapper = unsafe { OffsetPageTable::new(level_4_table, VirtAddr::new(0)) };
    let mut wrapper = UserFrameAllocator { frame_allocator };

    for index in 0..pages {
        let page =
            Page::<Size4KiB>::containing_address(VirtAddr::new(virtual_start + index * PAGE_SIZE));
        let frame =
            PhysFrame::containing_address(PhysAddr::new(physical_start + index * PAGE_SIZE));

        // SAFETY: `frame` is owned by the caller for this range, `page` is in
        // the fixed user stack range, and `wrapper` allocates new page-table
        // frames when the mapper needs them.
        unsafe {
            mapper
                .map_to(page, frame, flags, &mut wrapper)
                .expect("failed to map user stack page")
                .flush();
        }
    }
}

unsafe fn mark_page_table_path_user_accessible(virtual_address: VirtAddr) {
    let (level_4_frame, _) = Cr3::read();
    // SAFETY: The active page table is identity mapped by early paging.
    let level_4_table = unsafe { table_at(level_4_frame.start_address()) };

    let level_4_entry = &mut level_4_table[virtual_address.p4_index()];
    mark_entry_user_accessible(level_4_entry, "level-4 user entry page is not mapped");
    // SAFETY: The level-4 entry points to a present lower-level page table.
    let level_3_table = unsafe { table_at(level_4_entry.addr()) };

    let level_3_entry = &mut level_3_table[virtual_address.p3_index()];
    mark_entry_user_accessible(level_3_entry, "level-3 user entry page is not mapped");
    if level_3_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        return;
    }
    // SAFETY: The level-3 entry points to a present lower-level page table.
    let level_2_table = unsafe { table_at(level_3_entry.addr()) };

    let level_2_entry = &mut level_2_table[virtual_address.p2_index()];
    mark_entry_user_accessible(level_2_entry, "level-2 user entry page is not mapped");
    if level_2_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
        return;
    }
    // SAFETY: The level-2 entry points to a present lower-level page table.
    let level_1_table = unsafe { table_at(level_2_entry.addr()) };

    let level_1_entry = &mut level_1_table[virtual_address.p1_index()];
    mark_entry_user_accessible(level_1_entry, "level-1 user entry page is not mapped");
}

fn mark_entry_user_accessible(
    entry: &mut x86_64::structures::paging::page_table::PageTableEntry,
    panic_message: &str,
) {
    assert!(
        entry.flags().contains(PageTableFlags::PRESENT),
        "{panic_message}"
    );
    entry.set_flags(entry.flags() | PageTableFlags::USER_ACCESSIBLE);
}

unsafe fn table_at(address: PhysAddr) -> &'static mut PageTable {
    // SAFETY: ManaOS identity maps physical memory during early paging setup.
    unsafe { &mut *(address.as_u64() as *mut PageTable) }
}

struct UserFrameAllocator<'a> {
    frame_allocator: &'a mut BumpFrameAllocator,
}

// SAFETY: UserFrameAllocator delegates to BumpFrameAllocator, which returns each
// frame at most once from registered conventional memory regions.
unsafe impl FrameAllocator<Size4KiB> for UserFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.frame_allocator
            .allocate_frame()
            .map(|address| PhysFrame::containing_address(PhysAddr::new(address)))
    }
}
