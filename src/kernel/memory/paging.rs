use crate::kernel::memory::frame_allocator::BumpFrameAllocator;
use x86_64::{
    registers::control::Cr3,
    structures::paging::{Mapper, OffsetPageTable, PageTable, PageTableFlags, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

/// Initialize a new page table with identity mapping and switch to it.
pub unsafe fn init<'a>(
    frame_allocator: &mut BumpFrameAllocator,
    mmap_iter: impl Iterator<Item = &'a uefi::table::boot::MemoryDescriptor>,
    framebuffer_base: u64,
    framebuffer_size: u64,
) {
    // Allocate a frame for the new PML4 table
    let level_four_table_physical_address = frame_allocator
        .allocate_frame()
        .expect("OOM: failed to allocate level four page table frame");
    crate::serial_println!(
        "[paging] New PML4 at physical address: {:#x}",
        level_four_table_physical_address
    );

    if level_four_table_physical_address == 0 {
        panic!("PML4 allocated at physical address 0, which is treated as null!");
    }

    let level_four_table_ptr = level_four_table_physical_address as *mut PageTable;

    // Initialize PML4 to zero
    unsafe {
        core::ptr::write_bytes(level_four_table_ptr, 0, 1);
    }
    let level_four_table = unsafe { &mut *level_four_table_ptr };

    // Create an OffsetPageTable (with offset 0 for identity mapping)
    let mut mapper = OffsetPageTable::new(level_four_table, VirtAddr::new(0));

    // Identity map all relevant regions from the UEFI memory map
    for desc in mmap_iter {
        let start = desc.phys_start;
        let pages = desc.page_count;
        let end = start + pages * 4096;

        // Determine flags based on memory type (simplified)
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        // For now, we identity map everything in the memory map
        // to ensure we don't lose access to anything UEFI provided.
        for page_start in (start..end).step_by(4096) {
            let page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
                VirtAddr::new(page_start),
            );
            let frame = PhysFrame::containing_address(PhysAddr::new(page_start));

            // Map the page. We need to allocate frames for sub-tables (PDPT, PD, PT).
            let map_to_result = mapper.map_to(
                page,
                frame,
                flags,
                &mut FrameAllocWrapper { frame_allocator },
            );

            match map_to_result {
                Ok(t) => t.flush(),
                Err(e) => {
                    if !matches!(
                        e,
                        x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)
                    ) {
                        panic!("Failed to map page {:#x}: {:?}", page_start, e);
                    }
                }
            }
        }
    }

    // Identity map the frame buffer
    crate::serial_println!(
        "[paging] Mapping frame buffer: {:#x} (size: {} bytes)",
        framebuffer_base,
        framebuffer_size
    );
    let framebuffer_start_page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
        VirtAddr::new(framebuffer_base),
    );
    let framebuffer_end_page = x86_64::structures::paging::Page::<Size4KiB>::containing_address(
        VirtAddr::new(framebuffer_base + framebuffer_size - 1),
    );

    for page in x86_64::structures::paging::Page::range_inclusive(
        framebuffer_start_page,
        framebuffer_end_page,
    ) {
        let frame = PhysFrame::containing_address(PhysAddr::new(page.start_address().as_u64()));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        let map_to_result = mapper.map_to(
            page,
            frame,
            flags,
            &mut FrameAllocWrapper { frame_allocator },
        );

        match map_to_result {
            Ok(t) => t.flush(),
            Err(e) => {
                if !matches!(
                    e,
                    x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)
                ) {
                    panic!(
                        "Failed to map frame buffer page {:#x}: {:?}",
                        page.start_address().as_u64(),
                        e
                    );
                }
            }
        }
    }

    // Switch to the new page table
    let (old_level_four_table, flags) = Cr3::read();
    crate::serial_println!(
        "[paging] Switching page table: {:#x} -> {:#x}",
        old_level_four_table.start_address().as_u64(),
        level_four_table_physical_address
    );

    Cr3::write(
        PhysFrame::containing_address(PhysAddr::new(level_four_table_physical_address)),
        flags,
    );
    crate::serial_println!("[paging] Identity mapping complete.");
}

/// A wrapper to use our BumpFrameAllocator with x86_64's FrameAllocator trait.
struct FrameAllocWrapper<'a> {
    frame_allocator: &'a mut BumpFrameAllocator,
}

unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for FrameAllocWrapper<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.frame_allocator
            .allocate_frame()
            .map(|address| PhysFrame::containing_address(PhysAddr::new(address)))
    }
}
