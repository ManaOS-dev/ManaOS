use crate::kernel::memory::frame_allocator::BumpFrameAllocator;
use uefi::mem::memory_map::MemoryDescriptor;
use x86_64::{
    registers::control::Cr3,
    structures::paging::{
        Mapper, OffsetPageTable, PageTable, PageTableFlags, PhysFrame, Size4KiB, Translate,
    },
    PhysAddr, VirtAddr,
};

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
    framebuffer_base: u64,
    framebuffer_size: u64,
) {
    // SAFETY: The caller guarantees that the frame allocator returns valid page
    // table frames.
    let pml4_frame = unsafe { create_pml4(frame_allocator) };
    let pml4_table_ptr = pml4_frame.start_address().as_u64() as *mut PageTable;
    // SAFETY: pml4_frame was freshly allocated and zeroed by create_pml4.
    let pml4_table = unsafe { &mut *pml4_table_ptr };

    // SAFETY: ManaOS uses identity-mapped physical memory during early paging
    // setup, so a zero physical memory offset is valid here.
    let mut mapper = unsafe { OffsetPageTable::new(pml4_table, VirtAddr::new(0)) };

    // SAFETY: The caller provides the boot memory map and a valid allocator for
    // page-table frames.
    unsafe {
        map_memory_regions(&mut mapper, frame_allocator, mmap_iter);
        map_framebuffer(
            &mut mapper,
            frame_allocator,
            framebuffer_base,
            framebuffer_size,
        );
    }

    // Switch to the new page table
    // SAFETY: pml4_frame points to a valid level-4 page table built above.
    unsafe {
        Cr3::write(pml4_frame, x86_64::registers::control::Cr3Flags::empty());
    }
    crate::serial_println!("[paging] Identity mapping complete.");
}

unsafe fn create_pml4(frame_allocator: &mut BumpFrameAllocator) -> PhysFrame {
    let addr = frame_allocator
        .allocate_frame()
        .expect("OOM: failed to allocate PML4 frame");
    let frame = PhysFrame::containing_address(PhysAddr::new(addr));
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
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    for desc in mmap_iter {
        let start = desc.phys_start;
        let end = start + desc.page_count * 4096;

        let mut current_start = start;
        while current_start < end {
            if current_start % 0x200_000 == 0 && current_start + 0x200_000 <= end {
                let page = x86_64::structures::paging::Page::<x86_64::structures::paging::Size2MiB>::containing_address(VirtAddr::new(current_start));
                let frame = x86_64::structures::paging::PhysFrame::<
                    x86_64::structures::paging::Size2MiB,
                >::containing_address(PhysAddr::new(current_start));

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
                        "Failed to map 2MiB page {current_start:#x}: {e:?}"
                    ),
                }
                current_start += 0x200_000;
            } else {
                let page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
                    VirtAddr::new(current_start),
                );
                let frame = PhysFrame::containing_address(PhysAddr::new(current_start));

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
                        "Failed to map 4KiB page {current_start:#x}: {e:?}"
                    ),
                }
                current_start += 4096;
            }
        }
    }
}

unsafe fn map_framebuffer(
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut BumpFrameAllocator,
    framebuffer_base: u64,
    framebuffer_size: u64,
) {
    crate::serial_println!(
        "[paging] Mapping frame buffer: {:#x} (size: {} bytes)",
        framebuffer_base,
        framebuffer_size
    );
    let start_page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
        VirtAddr::new(framebuffer_base),
    );
    let end_page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(VirtAddr::new(
        framebuffer_base + framebuffer_size - 1,
    ));

    for page in x86_64::structures::paging::Page::range_inclusive(start_page, end_page) {
        let frame = PhysFrame::containing_address(PhysAddr::new(page.start_address().as_u64()));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

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
            .allocate_frame()
            .map(|address| PhysFrame::containing_address(PhysAddr::new(address)))
    }
}
