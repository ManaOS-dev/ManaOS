//! User address-space page-table ownership.

use super::{
    address::{PhysicalFrameRange, PhysicalFrameStart, UserVirtualAddress, VirtAddr},
    frame_allocator::{FrameRangeOwner, PhysicalFrameAllocator},
};
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::{
    registers::control::{Cr3, Cr3Flags},
    structures::paging::{
        mapper::TranslateResult, page_table::PageTableEntry, FrameAllocator, Mapper,
        OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, Translate,
    },
    PhysAddr as X86PhysAddr, VirtAddr as X86VirtAddr,
};

const PAGE_SIZE: u64 = 4096;
const USER_SPACE_END: usize = 0x0000_8000_0000_0000;
const PROCESS_USER_PML4_START: usize = 128;
const PROCESS_USER_PML4_END_EXCLUSIVE: usize = 256;
const USER_ADDRESS_SPACE_RECLAIM_PROBE: u64 = 0x0000_4000_0000_0000;

static KERNEL_LEVEL_4_FRAME: AtomicU64 = AtomicU64::new(0);

/// Page counts reclaimed while destroying one user address space.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UserAddressSpaceReclaim {
    user_pages: u64,
    page_table_pages: u64,
}

impl UserAddressSpaceReclaim {
    /// Return the number of user data pages returned to the frame allocator.
    pub const fn user_pages(self) -> u64 {
        self.user_pages
    }

    /// Return the number of page-table pages returned to the frame allocator.
    pub const fn page_table_pages(self) -> u64 {
        self.page_table_pages
    }
}

/// Page-table root for one user address space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserAddressSpace {
    level_4_frame: PhysicalFrameStart,
}

impl UserAddressSpace {
    /// Return the physical frame containing the level-4 page table.
    pub fn level_4_frame(self) -> PhysicalFrameStart {
        self.level_4_frame
    }

    /// Map one 4 KiB user page in this address space.
    ///
    /// # Panics
    ///
    /// Panics if the user virtual address is not page-aligned, if the target
    /// page is already mapped, or if page-table frame allocation fails.
    pub fn map_user_page(
        self,
        frame_allocator: &mut PhysicalFrameAllocator,
        virtual_address: UserVirtualAddress,
        physical_start: PhysicalFrameStart,
        flags: PageTableFlags,
    ) {
        assert!(
            virtual_address.as_u64().is_multiple_of(PAGE_SIZE),
            "user page virtual address must be 4KiB aligned"
        );

        let level_4_table = level_4_table_from_frame(self.level_4_frame);
        // SAFETY: ManaOS keeps physical memory identity mapped in every kernel
        // address-space template, so the page-table frame is directly
        // reachable while building user mappings.
        let mut mapper = unsafe { OffsetPageTable::new(level_4_table, X86VirtAddr::new(0)) };
        let page = Page::<Size4KiB>::containing_address(X86VirtAddr::new(virtual_address.as_u64()));
        let frame = PhysFrame::containing_address(X86PhysAddr::new(physical_start.as_u64()));

        // SAFETY: The caller owns `physical_start` for the user mapping, and
        // this address-space root owns the page-table hierarchy being mutated.
        unsafe {
            mapper
                .map_to(
                    page,
                    frame,
                    flags,
                    &mut AddressSpaceFrameAllocator { frame_allocator },
                )
                .expect("failed to map user address-space page")
                .flush();
        }
    }

    /// Unmap one 4 KiB user page and return its frame to the expected owner.
    ///
    /// Returns `false` when the page is already unmapped.
    ///
    /// # Panics
    ///
    /// Panics if the user virtual address is not page-aligned, the mapped
    /// physical frame is not 4 KiB-aligned, or the frame allocator rejects the
    /// expected owner.
    pub fn unmap_user_page_for(
        self,
        frame_allocator: &mut PhysicalFrameAllocator,
        virtual_address: UserVirtualAddress,
        owner: FrameRangeOwner,
    ) -> bool {
        assert!(
            virtual_address.as_u64().is_multiple_of(PAGE_SIZE),
            "user page virtual address must be 4KiB aligned"
        );

        let level_4_table = level_4_table_from_frame(self.level_4_frame);
        // SAFETY: ManaOS keeps physical memory identity mapped in every kernel
        // address-space template, so the page-table frame is directly
        // reachable while removing user mappings.
        let mut mapper = unsafe { OffsetPageTable::new(level_4_table, X86VirtAddr::new(0)) };
        let page = Page::<Size4KiB>::containing_address(X86VirtAddr::new(virtual_address.as_u64()));
        let Ok((frame, flush)) = mapper.unmap(page) else {
            return false;
        };
        flush.flush();

        let physical_start = PhysicalFrameStart::new(frame.start_address().as_u64())
            .expect("unmapped user page frame must be 4KiB aligned");
        let physical_range =
            PhysicalFrameRange::new(physical_start, 1).expect("single-frame range must be valid");
        assert!(
            frame_allocator.free_frames_for(physical_range, owner),
            "unmapped user page frame owner did not match expected owner"
        );
        true
    }

    /// Return whether the range is mapped as readable non-executable user data.
    pub fn is_user_range_mapped_readable(self, user_pointer: usize, length: usize) -> bool {
        self.validate_user_mapping(user_pointer, length, PageTableFlags::NO_EXECUTE)
    }

    /// Return whether the range is mapped as writable non-executable user data.
    pub fn is_user_range_mapped_writable(self, user_pointer: usize, length: usize) -> bool {
        self.validate_user_mapping(
            user_pointer,
            length,
            PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE,
        )
    }

    /// Verify representative kernel and user mapping permissions in this space.
    pub fn verify_kernel_user_mapping_permissions(
        self,
        kernel_pointer: usize,
        user_stack_pointer: usize,
        user_entry_pointer: usize,
    ) -> bool {
        let Some(kernel_flags) =
            self.mapping_flags_for_address(VirtAddr::new(kernel_pointer as u64))
        else {
            return false;
        };
        if !kernel_flags.contains(PageTableFlags::PRESENT)
            || kernel_flags.contains(PageTableFlags::USER_ACCESSIBLE)
        {
            return false;
        }

        let Some(user_stack_flags) =
            self.mapping_flags_for_address(VirtAddr::new(user_stack_pointer as u64))
        else {
            return false;
        };
        if !user_stack_flags.contains(PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE)
            || !user_stack_flags.contains(PageTableFlags::WRITABLE)
            || !user_stack_flags.contains(PageTableFlags::NO_EXECUTE)
        {
            return false;
        }

        let Some(user_entry_flags) =
            self.mapping_flags_for_address(VirtAddr::new(user_entry_pointer as u64))
        else {
            return false;
        };
        user_entry_flags.contains(PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE)
            && !user_entry_flags.contains(PageTableFlags::WRITABLE)
            && !user_entry_flags.contains(PageTableFlags::NO_EXECUTE)
    }

    /// Verify syscall user-data pointer permission enforcement in this space.
    pub fn verify_syscall_user_data_permissions(
        self,
        user_stack_pointer: usize,
        user_entry_pointer: usize,
    ) -> bool {
        self.is_user_range_mapped_readable(user_stack_pointer, 1)
            && self.is_user_range_mapped_writable(user_stack_pointer, 1)
            && !self.is_user_range_mapped_readable(user_entry_pointer, 1)
            && !self.is_user_range_mapped_writable(user_entry_pointer, 1)
    }

    fn validate_user_mapping(
        self,
        user_pointer: usize,
        length: usize,
        required_flags: PageTableFlags,
    ) -> bool {
        if length == 0 {
            return true;
        }

        if user_pointer == 0 {
            return false;
        }

        let Some(last_byte_pointer) = user_pointer.checked_add(length - 1) else {
            return false;
        };
        if last_byte_pointer >= USER_SPACE_END {
            return false;
        }

        let first_page_start = VirtAddr::new(user_pointer as u64).align_down_to_page();
        let last_page_start = VirtAddr::new(last_byte_pointer as u64).align_down_to_page();

        let mut page_start = first_page_start;
        loop {
            if !self.is_page_mapped_with_flags(page_start, required_flags) {
                return false;
            }

            if page_start == last_page_start {
                return true;
            }

            let Some(next_page_start) = page_start.checked_add(PAGE_SIZE) else {
                return false;
            };
            page_start = next_page_start;
        }
    }

    fn is_page_mapped_with_flags(
        self,
        page_start: VirtAddr,
        required_flags: PageTableFlags,
    ) -> bool {
        let required_flags =
            required_flags | PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

        match self.mapping_flags_for_address(page_start) {
            Some(flags) => flags.contains(required_flags),
            None => false,
        }
    }

    fn mapping_flags_for_address(self, address: VirtAddr) -> Option<PageTableFlags> {
        let level_4_table = level_4_table_from_frame(self.level_4_frame);
        // SAFETY: The address-space root frame is identity mapped for page-table
        // walks while the kernel is active.
        let mapper = unsafe { OffsetPageTable::new(level_4_table, X86VirtAddr::new(0)) };

        match mapper.translate(X86VirtAddr::new(address.as_u64())) {
            TranslateResult::Mapped { flags, .. } => Some(flags),
            TranslateResult::NotMapped | TranslateResult::InvalidFrameAddress(_) => None,
        }
    }
}

/// Record the kernel address-space root after paging is initialized.
pub fn initialize_kernel_address_space(level_4_frame: PhysicalFrameStart) {
    KERNEL_LEVEL_4_FRAME.store(level_4_frame.as_u64(), Ordering::Release);
}

/// Create a user address space with shared kernel mappings and empty user slots.
///
/// # Panics
///
/// Panics if a page-table root frame cannot be allocated.
pub fn create_user_address_space(frame_allocator: &mut PhysicalFrameAllocator) -> UserAddressSpace {
    let level_4_frame = frame_allocator
        .allocate_frame_for(FrameRangeOwner::PageTable)
        .expect("OOM: failed to allocate user address-space PML4 frame");
    let active_level_4_table = active_level_4_table();
    let user_level_4_table = level_4_table_from_frame(level_4_frame);
    user_level_4_table.zero();

    for index in 0..512 {
        user_level_4_table[index] = active_level_4_table[index].clone();
    }
    for index in PROCESS_USER_PML4_START..PROCESS_USER_PML4_END_EXCLUSIVE {
        user_level_4_table[index].set_unused();
    }

    UserAddressSpace { level_4_frame }
}

/// Switch the CPU to a user address space.
pub fn switch_to_user_address_space(address_space: UserAddressSpace) {
    switch_to_level_4(address_space.level_4_frame);
}

/// Switch the CPU back to the kernel address space.
///
/// # Panics
///
/// Panics if paging has not recorded the kernel address-space root.
pub fn switch_to_kernel_address_space() {
    let raw_frame = KERNEL_LEVEL_4_FRAME.load(Ordering::Acquire);
    let level_4_frame =
        PhysicalFrameStart::new(raw_frame).expect("kernel address space must be initialized");
    switch_to_level_4(level_4_frame);
}

/// Destroy a user address space and return its owned frames to the allocator.
///
/// # Panics
///
/// Panics if a user-window page table contains a huge-page mapping or if a
/// mapped user data frame is not tracked as user stack or user ELF memory.
pub fn destroy_user_address_space(
    frame_allocator: &mut PhysicalFrameAllocator,
    address_space: UserAddressSpace,
) -> UserAddressSpaceReclaim {
    switch_to_kernel_address_space();

    let mut reclaim = UserAddressSpaceReclaim::default();
    let level_4_table = level_4_table_from_frame(address_space.level_4_frame);
    for index in PROCESS_USER_PML4_START..PROCESS_USER_PML4_END_EXCLUSIVE {
        reclaim_user_child_table(frame_allocator, &mut level_4_table[index], &mut reclaim);
    }
    level_4_table.zero();
    free_page_table_frame(frame_allocator, address_space.level_4_frame, &mut reclaim);
    reclaim
}

/// Verify that a fresh user address-space template isolates user mappings.
pub fn verify_user_address_space_template(
    frame_allocator: &mut PhysicalFrameAllocator,
    kernel_pointer: usize,
) -> bool {
    let address_space = create_user_address_space(frame_allocator);
    let kernel_mapping_present = address_space
        .mapping_flags_for_address(VirtAddr::new(kernel_pointer as u64))
        .is_some_and(|flags| {
            flags.contains(PageTableFlags::PRESENT)
                && !flags.contains(PageTableFlags::USER_ACCESSIBLE)
        });
    let process_user_window_empty = (PROCESS_USER_PML4_START..PROCESS_USER_PML4_END_EXCLUSIVE)
        .all(|index| level_4_table_from_frame(address_space.level_4_frame)[index].is_unused());
    let reclaim = destroy_user_address_space(frame_allocator, address_space);

    kernel_mapping_present
        && process_user_window_empty
        && reclaim.user_pages() == 0
        && reclaim.page_table_pages() == 1
}

/// Verify that destroying a user address space reclaims mapped user frames.
pub fn verify_user_address_space_reclaim(
    frame_allocator: &mut PhysicalFrameAllocator,
) -> Option<UserAddressSpaceReclaim> {
    let address_space = create_user_address_space(frame_allocator);
    let physical_start = frame_allocator.allocate_frame_for(FrameRangeOwner::UserElf)?;
    let virtual_address = UserVirtualAddress::new(USER_ADDRESS_SPACE_RECLAIM_PROBE)
        .expect("user address-space reclaim probe must be a valid user address");
    address_space.map_user_page(
        frame_allocator,
        virtual_address,
        physical_start,
        PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE,
    );

    let reclaim = destroy_user_address_space(frame_allocator, address_space);
    if reclaim.user_pages() == 1 && reclaim.page_table_pages() == 4 {
        Some(reclaim)
    } else {
        None
    }
}

fn active_level_4_table() -> &'static mut PageTable {
    let (level_4_frame, _) = Cr3::read();
    let level_4_table = level_4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: ManaOS keeps active page tables identity mapped, so the CR3
    // frame physical address is usable as a kernel virtual address.
    unsafe { &mut *level_4_table }
}

fn level_4_table_from_frame(level_4_frame: PhysicalFrameStart) -> &'static mut PageTable {
    let level_4_table = level_4_frame.as_u64() as *mut PageTable;
    // SAFETY: Page-table root frames are allocated from identity-mapped
    // physical memory and retained by their address-space owner.
    unsafe { &mut *level_4_table }
}

fn switch_to_level_4(level_4_frame: PhysicalFrameStart) {
    let frame = PhysFrame::containing_address(X86PhysAddr::new(level_4_frame.as_u64()));
    // SAFETY: `level_4_frame` points to a valid PML4 built by ManaOS paging
    // code and shares the kernel mappings required after the switch.
    unsafe {
        Cr3::write(frame, Cr3Flags::empty());
    }
}

fn reclaim_user_child_table(
    frame_allocator: &mut PhysicalFrameAllocator,
    entry: &mut PageTableEntry,
    reclaim: &mut UserAddressSpaceReclaim,
) {
    if entry.is_unused() {
        return;
    }
    assert!(
        !entry.flags().contains(PageTableFlags::HUGE_PAGE),
        "user address-space PML4 entry must not be a huge page"
    );

    let child_frame = entry_frame_start(entry);
    let child_table = level_4_table_from_frame(child_frame);
    reclaim_level_3_table(frame_allocator, child_table, reclaim);
    child_table.zero();
    entry.set_unused();
    free_page_table_frame(frame_allocator, child_frame, reclaim);
}

fn reclaim_level_3_table(
    frame_allocator: &mut PhysicalFrameAllocator,
    table: &mut PageTable,
    reclaim: &mut UserAddressSpaceReclaim,
) {
    for entry in table.iter_mut() {
        if entry.is_unused() {
            continue;
        }
        assert!(
            !entry.flags().contains(PageTableFlags::HUGE_PAGE),
            "user address-space PDP entry must not be a huge page"
        );

        let child_frame = entry_frame_start(entry);
        let child_table = level_4_table_from_frame(child_frame);
        reclaim_level_2_table(frame_allocator, child_table, reclaim);
        child_table.zero();
        entry.set_unused();
        free_page_table_frame(frame_allocator, child_frame, reclaim);
    }
}

fn reclaim_level_2_table(
    frame_allocator: &mut PhysicalFrameAllocator,
    table: &mut PageTable,
    reclaim: &mut UserAddressSpaceReclaim,
) {
    for entry in table.iter_mut() {
        if entry.is_unused() {
            continue;
        }
        assert!(
            !entry.flags().contains(PageTableFlags::HUGE_PAGE),
            "user address-space PD entry must not be a huge page"
        );

        let child_frame = entry_frame_start(entry);
        let child_table = level_4_table_from_frame(child_frame);
        reclaim_level_1_table(frame_allocator, child_table, reclaim);
        child_table.zero();
        entry.set_unused();
        free_page_table_frame(frame_allocator, child_frame, reclaim);
    }
}

fn reclaim_level_1_table(
    frame_allocator: &mut PhysicalFrameAllocator,
    table: &mut PageTable,
    reclaim: &mut UserAddressSpaceReclaim,
) {
    for entry in table.iter_mut() {
        if entry.is_unused() {
            continue;
        }
        assert!(
            !entry.flags().contains(PageTableFlags::HUGE_PAGE),
            "user address-space PT entry must not be a huge page"
        );

        let data_frame = entry_frame_start(entry);
        entry.set_unused();
        free_user_data_frame(frame_allocator, data_frame, reclaim);
    }
}

fn entry_frame_start(entry: &PageTableEntry) -> PhysicalFrameStart {
    let frame = entry
        .frame()
        .expect("present user address-space entry must reference a 4KiB frame");
    PhysicalFrameStart::new(frame.start_address().as_u64())
        .expect("page-table entry frame must be 4KiB aligned")
}

fn free_user_data_frame(
    frame_allocator: &mut PhysicalFrameAllocator,
    frame: PhysicalFrameStart,
    reclaim: &mut UserAddressSpaceReclaim,
) {
    let range = one_frame_range(frame);
    if !frame_allocator.free_frames_for(range, FrameRangeOwner::UserElf)
        && !frame_allocator.free_frames_for(range, FrameRangeOwner::UserStack)
        && !frame_allocator.free_frames_for(range, FrameRangeOwner::UserHeap)
    {
        panic!(
            "user address-space data frame {:#x} was not tracked as user-owned memory",
            frame.as_u64()
        );
    }
    reclaim.user_pages = reclaim.user_pages.saturating_add(1);
}

fn free_page_table_frame(
    frame_allocator: &mut PhysicalFrameAllocator,
    frame: PhysicalFrameStart,
    reclaim: &mut UserAddressSpaceReclaim,
) {
    assert!(
        frame_allocator.free_frames_for(one_frame_range(frame), FrameRangeOwner::PageTable),
        "user address-space page-table frame was not tracked as a page table"
    );
    reclaim.page_table_pages = reclaim.page_table_pages.saturating_add(1);
}

fn one_frame_range(frame: PhysicalFrameStart) -> PhysicalFrameRange {
    PhysicalFrameRange::new(frame, 1).expect("single-frame range must be non-empty")
}

struct AddressSpaceFrameAllocator<'a> {
    frame_allocator: &'a mut PhysicalFrameAllocator,
}

// SAFETY: AddressSpaceFrameAllocator delegates to PhysicalFrameAllocator, which
// returns each page-table frame at most once until explicitly freed.
unsafe impl FrameAllocator<Size4KiB> for AddressSpaceFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.frame_allocator
            .allocate_frame_for(FrameRangeOwner::PageTable)
            .map(|address| PhysFrame::containing_address(X86PhysAddr::new(address.as_u64())))
    }
}
