use crate::kernel::memory::{
    address::{FramebufferPhysicalRange, PhysAddr as KernelPhysAddr, VirtAddr as KernelVirtAddr},
    frame_allocator::{BumpFrameAllocator, FrameRangeOwner},
};
use uefi::mem::memory_map::{MemoryDescriptor, MemoryType};
use x86_64::{
    registers::{
        control::Cr3,
        model_specific::{Efer, EferFlags},
    },
    structures::paging::{
        mapper::TranslateResult, Mapper, OffsetPageTable, PageTable, PageTableFlags, PhysFrame,
        Size4KiB, Translate,
    },
    PhysAddr as X86PhysAddr, VirtAddr as X86VirtAddr,
};

const PAGE_SIZE: u64 = 4096;
const USER_SPACE_END: usize = 0x0000_8000_0000_0000;

/// Initialize a new page table with identity mapping and switch to it.
///
/// # Safety
///
/// The provided frame allocator must return valid, unused, page-aligned physical
/// frames. The memory map iterator must describe memory that can be identity
/// mapped, and the framebuffer range must come from the active graphics mode.
pub unsafe fn init<'a>(
    frame_allocator: &mut BumpFrameAllocator,
    mmap_iter: impl Iterator<Item = &'a MemoryDescriptor>,
    framebuffer_range: FramebufferPhysicalRange,
) {
    // SAFETY: Setting NXE enables honoring the NO_EXECUTE page-table flag while
    // preserving all other EFER bits.
    unsafe {
        Efer::update(|flags| flags.insert(EferFlags::NO_EXECUTE_ENABLE));
    }

    // SAFETY: The caller guarantees that the frame allocator returns valid page
    // table frames.
    let pml4_frame = unsafe { create_pml4(frame_allocator) };
    let pml4_table_ptr = pml4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: pml4_frame was freshly allocated and zeroed by create_pml4.
    let pml4_table = unsafe { &mut *pml4_table_ptr };

    // SAFETY: ManaOS uses identity-mapped physical memory during early paging
    // setup, so a zero physical memory offset is valid here.
    let mut mapper = unsafe { OffsetPageTable::new(pml4_table, X86VirtAddr::new(0)) };

    // SAFETY: The caller provides the boot memory map and a valid allocator for
    // page-table frames.
    unsafe {
        map_memory_regions(&mut mapper, frame_allocator, mmap_iter);
        map_framebuffer(&mut mapper, frame_allocator, framebuffer_range);
    }

    // Switch to the new page table
    // SAFETY: pml4_frame points to a valid level-4 page table built above.
    unsafe {
        Cr3::write(pml4_frame, x86_64::registers::control::Cr3Flags::empty());
    }
    crate::log_info!("paging", "Identity mapping complete.");
}

/// Return whether the whole user range is mapped as readable non-executable user data.
pub fn is_user_range_mapped_readable(user_pointer: usize, length: usize) -> bool {
    validate_user_mapping(user_pointer, length, PageTableFlags::NO_EXECUTE)
}

/// Return whether the whole user range is mapped as writable non-executable user data.
pub fn is_user_range_mapped_writable(user_pointer: usize, length: usize) -> bool {
    validate_user_mapping(
        user_pointer,
        length,
        PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE,
    )
}

/// Verify representative kernel and user mapping permissions.
///
/// The kernel pointer must be mapped but not user-accessible. The user stack
/// pointer must be mapped as writable, user-accessible, and non-executable. The
/// user entry pointer must be mapped as user-accessible executable code and not
/// writable.
pub fn verify_kernel_user_mapping_permissions(
    kernel_pointer: usize,
    user_stack_pointer: usize,
    user_entry_pointer: usize,
) -> bool {
    let Some(kernel_flags) = mapping_flags_for_address(KernelVirtAddr::new(kernel_pointer as u64))
    else {
        return false;
    };
    if !kernel_flags.contains(PageTableFlags::PRESENT)
        || kernel_flags.contains(PageTableFlags::USER_ACCESSIBLE)
    {
        return false;
    }

    let Some(user_stack_flags) =
        mapping_flags_for_address(KernelVirtAddr::new(user_stack_pointer as u64))
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
        mapping_flags_for_address(KernelVirtAddr::new(user_entry_pointer as u64))
    else {
        return false;
    };
    user_entry_flags.contains(PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE)
        && !user_entry_flags.contains(PageTableFlags::WRITABLE)
        && !user_entry_flags.contains(PageTableFlags::NO_EXECUTE)
}

/// Verify syscall user-data pointer permission enforcement.
///
/// The writable stack pointer must pass readable and writable user-data checks.
/// The executable user entry pointer must fail both checks because syscall data
/// buffers are required to be non-executable.
pub fn verify_syscall_user_data_permissions(
    user_stack_pointer: usize,
    user_entry_pointer: usize,
) -> bool {
    is_user_range_mapped_readable(user_stack_pointer, 1)
        && is_user_range_mapped_writable(user_stack_pointer, 1)
        && !is_user_range_mapped_readable(user_entry_pointer, 1)
        && !is_user_range_mapped_writable(user_entry_pointer, 1)
}

/// Identity-map a kernel MMIO range as writable and uncached.
///
/// # Panics
///
/// Panics if the range is empty, overflows, or page-table mapping fails.
///
/// # Safety
///
/// The caller must ensure the physical range belongs to an MMIO device and that
/// mapping it into the kernel address space does not alias regular RAM.
pub unsafe fn map_kernel_mmio_range(
    frame_allocator: &mut BumpFrameAllocator,
    physical_start: KernelPhysAddr,
    size: u64,
) {
    assert!(size > 0, "MMIO mapping size must be non-zero");

    let start_page_address = physical_start.align_down_to_page();
    let end_address = physical_start
        .checked_add(size - 1)
        .expect("MMIO mapping end address overflowed");
    let end_page_address = end_address.align_down_to_page();
    let page_count = ((end_page_address.as_u64() - start_page_address.as_u64()) / PAGE_SIZE) + 1;
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_CACHE
        | PageTableFlags::NO_EXECUTE;

    let (level_4_frame, _) = Cr3::read();
    let level_4_table = level_4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: ManaOS keeps active page tables identity mapped, so the physical
    // address from CR3 is directly usable as a kernel virtual address.
    let level_4_table = unsafe { &mut *level_4_table };
    // SAFETY: The active address space uses an identity physical memory offset.
    let mut mapper = unsafe { OffsetPageTable::new(level_4_table, X86VirtAddr::new(0)) };

    // SAFETY: The caller guarantees this is an MMIO range, and this helper
    // identity maps exactly the pages covering that range as uncached memory.
    unsafe {
        map_identity_pages(
            &mut mapper,
            frame_allocator,
            start_page_address,
            page_count,
            flags,
        );
    }
}

fn validate_user_mapping(
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

    let first_page_start = KernelVirtAddr::new(user_pointer as u64).align_down_to_page();
    let last_page_start = KernelVirtAddr::new(last_byte_pointer as u64).align_down_to_page();

    let (level_4_frame, _) = Cr3::read();
    let level_4_table = level_4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: ManaOS keeps active page tables identity mapped, so the physical
    // address from CR3 is directly usable as a kernel virtual address.
    let level_4_table = unsafe { &mut *level_4_table };
    // SAFETY: The active address space uses an identity physical memory offset.
    let mapper = unsafe { OffsetPageTable::new(level_4_table, X86VirtAddr::new(0)) };

    let mut page_start = first_page_start;
    loop {
        if !is_page_mapped_with_flags(&mapper, page_start, required_flags) {
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
    mapper: &OffsetPageTable,
    page_start: KernelVirtAddr,
    required_flags: PageTableFlags,
) -> bool {
    let required_flags = required_flags | PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    match mapper.translate(X86VirtAddr::new(page_start.as_u64())) {
        TranslateResult::Mapped { flags, .. } => flags.contains(required_flags),
        TranslateResult::NotMapped | TranslateResult::InvalidFrameAddress(_) => false,
    }
}

fn mapping_flags_for_address(address: KernelVirtAddr) -> Option<PageTableFlags> {
    let (level_4_frame, _) = Cr3::read();
    let level_4_table = level_4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: ManaOS keeps active page tables identity mapped, so the physical
    // address from CR3 is directly usable as a kernel virtual address.
    let level_4_table = unsafe { &mut *level_4_table };
    // SAFETY: The active address space uses an identity physical memory offset.
    let mapper = unsafe { OffsetPageTable::new(level_4_table, X86VirtAddr::new(0)) };

    match mapper.translate(X86VirtAddr::new(address.as_u64())) {
        TranslateResult::Mapped { flags, .. } => Some(flags),
        TranslateResult::NotMapped | TranslateResult::InvalidFrameAddress(_) => None,
    }
}

unsafe fn create_pml4(frame_allocator: &mut BumpFrameAllocator) -> PhysFrame {
    let pml4_frame_start = frame_allocator
        .allocate_frame_for(FrameRangeOwner::PageTable)
        .expect("OOM: failed to allocate PML4 frame");
    let frame = PhysFrame::containing_address(X86PhysAddr::new(pml4_frame_start.as_u64()));
    let ptr = frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: ptr points to a freshly allocated 4KiB page table frame.
    unsafe {
        core::ptr::write_bytes(ptr, 0, 1);
    }
    frame
}

unsafe fn map_memory_regions<'a>(
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut BumpFrameAllocator,
    mmap_iter: impl Iterator<Item = &'a MemoryDescriptor>,
) {
    let mut executable_pages = 0_u64;
    let mut non_executable_pages = 0_u64;
    for desc in mmap_iter {
        let flags = memory_region_flags(desc.ty);
        if flags.contains(PageTableFlags::NO_EXECUTE) {
            non_executable_pages = non_executable_pages.saturating_add(desc.page_count);
        } else {
            executable_pages = executable_pages.saturating_add(desc.page_count);
        }
        let start = KernelPhysAddr::new(desc.phys_start);
        let size = desc
            .page_count
            .checked_mul(4096)
            .expect("memory map region size overflowed");
        let end = start
            .checked_add(size)
            .expect("memory map region end address overflowed");

        let mut current_start = start;
        while current_start.as_u64() < end.as_u64() {
            let next_huge_page_start = current_start
                .checked_add(0x200_000)
                .expect("2MiB mapping address overflowed");
            let current_start_raw = current_start.as_u64();
            if current_start_raw.is_multiple_of(0x200_000)
                && next_huge_page_start.as_u64() <= end.as_u64()
            {
                let page = x86_64::structures::paging::Page::<
                    x86_64::structures::paging::Size2MiB,
                >::containing_address(X86VirtAddr::new(current_start_raw));
                let frame = x86_64::structures::paging::PhysFrame::<
                    x86_64::structures::paging::Size2MiB,
                >::containing_address(X86PhysAddr::new(
                    current_start_raw,
                ));

                match mapper.map_to(
                    page,
                    frame,
                    flags | PageTableFlags::HUGE_PAGE,
                    &mut FrameAllocWrapper { frame_allocator },
                ) {
                    Ok(t) => t.flush(),
                    Err(e) => assert!(
                        matches!(
                            e,
                            x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)
                        ),
                        "Failed to map 2MiB page {current_start_raw:#x}: {e:?}"
                    ),
                }
                current_start = next_huge_page_start;
            } else {
                let page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
                    X86VirtAddr::new(current_start_raw),
                );
                let frame = PhysFrame::containing_address(X86PhysAddr::new(current_start_raw));

                match mapper.map_to(
                    page,
                    frame,
                    flags,
                    &mut FrameAllocWrapper { frame_allocator },
                ) {
                    Ok(t) => t.flush(),
                    Err(e) => assert!(
                        matches!(
                            e,
                            x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)
                        ),
                        "Failed to map 4KiB page {current_start_raw:#x}: {e:?}"
                    ),
                }
                current_start = current_start
                    .checked_add(4096)
                    .expect("4KiB mapping address overflowed");
            }
        }
    }

    crate::log_info!(
        "paging",
        "Identity mapping permissions: executable_pages={} non_executable_pages={}",
        executable_pages,
        non_executable_pages
    );
}

fn memory_region_flags(memory_type: MemoryType) -> PageTableFlags {
    let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    if !is_executable_memory_type(memory_type) {
        flags |= PageTableFlags::NO_EXECUTE;
    }
    flags
}

fn is_executable_memory_type(memory_type: MemoryType) -> bool {
    matches!(
        memory_type,
        MemoryType::LOADER_CODE
            | MemoryType::BOOT_SERVICES_CODE
            | MemoryType::RUNTIME_SERVICES_CODE
    )
}

unsafe fn map_identity_pages(
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut BumpFrameAllocator,
    start_address: KernelPhysAddr,
    page_count: u64,
    flags: PageTableFlags,
) {
    for index in 0..page_count {
        let offset = index
            .checked_mul(PAGE_SIZE)
            .expect("identity mapping offset overflowed");
        let address = start_address
            .checked_add(offset)
            .expect("identity mapping address overflowed");
        let raw_address = address.as_u64();
        let page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
            X86VirtAddr::new(raw_address),
        );
        let frame = PhysFrame::containing_address(X86PhysAddr::new(raw_address));

        if let x86_64::structures::paging::mapper::TranslateResult::Mapped { .. } =
            mapper.translate(page.start_address())
        {
            continue;
        }

        match mapper.map_to(
            page,
            frame,
            flags,
            &mut FrameAllocWrapper { frame_allocator },
        ) {
            Ok(t) => t.flush(),
            Err(e) => assert!(
                matches!(
                    e,
                    x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)
                ),
                "Failed to map identity page {raw_address:#x}: {e:?}"
            ),
        }
    }
}

unsafe fn map_framebuffer(
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut BumpFrameAllocator,
    framebuffer_range: FramebufferPhysicalRange,
) {
    let framebuffer_start = framebuffer_range.start();
    let framebuffer_size = framebuffer_range.byte_len();
    let framebuffer_base = framebuffer_start.as_u64();
    crate::log_info!(
        "paging",
        "Mapping framebuffer: base={:#x} size={} bytes",
        framebuffer_base,
        framebuffer_size
    );
    let start_page_address = KernelVirtAddr::new(framebuffer_start.align_down_to_page().as_u64());
    let start_page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
        X86VirtAddr::new(start_page_address.as_u64()),
    );
    let end_address = framebuffer_start
        .checked_add(framebuffer_size - 1)
        .expect("framebuffer end address overflowed");
    let end_page_address = KernelVirtAddr::new(end_address.align_down_to_page().as_u64());
    let end_page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
        X86VirtAddr::new(end_page_address.as_u64()),
    );

    for page in x86_64::structures::paging::Page::range_inclusive(start_page, end_page) {
        let frame_address = KernelPhysAddr::new(page.start_address().as_u64());
        let frame = PhysFrame::containing_address(X86PhysAddr::new(frame_address.as_u64()));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;

        if let x86_64::structures::paging::mapper::TranslateResult::Mapped { .. } =
            mapper.translate(page.start_address())
        {
            continue;
        }

        match mapper.map_to(
            page,
            frame,
            flags,
            &mut FrameAllocWrapper { frame_allocator },
        ) {
            Ok(t) => t.flush(),
            Err(e) => assert!(
                matches!(
                    e,
                    x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)
                ),
                "Failed to map frame buffer page {:#x}: {e:?}",
                page.start_address().as_u64()
            ),
        }
    }
}

/// A wrapper to use our `BumpFrameAllocator` with `x86_64`'s `FrameAllocator` trait.
struct FrameAllocWrapper<'a> {
    frame_allocator: &'a mut BumpFrameAllocator,
}

// SAFETY: FrameAllocWrapper delegates to BumpFrameAllocator, which returns each
// frame at most once from registered conventional memory regions.
unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for FrameAllocWrapper<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.frame_allocator
            .allocate_frame_for(FrameRangeOwner::PageTable)
            .map(|address| PhysFrame::containing_address(X86PhysAddr::new(address.as_u64())))
    }
}
