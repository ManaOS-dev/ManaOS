//! UEFI boot-services helpers used before entering the kernel phase.

use crate::kernel;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::system;
use uefi::table::cfg::ConfigTableEntry;
use uefi::{
    boot,
    mem::memory_map::{MemoryDescriptor, MemoryType},
};

/// Load a file from the EFI System Partition into boot-owned memory.
pub(crate) fn load_file(path: &str) -> &'static mut [u8] {
    use uefi::proto::media::file::FileInfo;

    let fs_handle = boot::get_handle_for_protocol::<SimpleFileSystem>()
        .expect("Failed to get SimpleFileSystem handle");
    let mut fs = boot::open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
        .expect("Failed to open SimpleFileSystem");

    let mut root = fs.open_volume().expect("Failed to open volume");

    let mut path_buffer = [0u16; 128];
    let path_cstr = uefi::CStr16::from_str_with_buf(path, &mut path_buffer)
        .expect("Failed to convert path to CStr16");

    let mut file = root
        .open(path_cstr, FileMode::Read, FileAttribute::empty())
        .expect("Failed to open file")
        .into_regular_file()
        .expect("Not a regular file");

    let mut info_buf = [0u8; 256];
    let info = file
        .get_info::<FileInfo>(&mut info_buf)
        .expect("Failed to get file info");
    let size = usize::try_from(info.file_size()).expect("File too large");

    let ptr = boot::allocate_pool(MemoryType::LOADER_DATA, size)
        .expect("Failed to allocate pool for file");

    // SAFETY: allocate_pool returned a valid pointer to a LOADER_DATA buffer of
    // exactly size bytes, and the buffer remains owned by the boot phase.
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr.as_ptr(), size) };
    file.read(buffer).expect("Failed to read file");

    buffer
}

/// Return the active GOP framebuffer mode selected by UEFI.
pub(crate) fn get_framebuffer_info() -> kernel::driver::display::framebuffer::FrameBufferInfo {
    let graphics_output_handle = boot::get_handle_for_protocol::<GraphicsOutput>()
        .expect("GraphicsOutput handle is required for ManaOS framebuffer setup");
    let mut graphics_output =
        boot::open_protocol_exclusive::<GraphicsOutput>(graphics_output_handle)
            .expect("GraphicsOutput protocol is required for ManaOS framebuffer setup");
    kernel::driver::display::framebuffer::get_info(&mut graphics_output)
}

/// Find the UEFI ACPI root pointer from the system configuration table.
pub(crate) fn find_acpi_root_pointer() -> Option<kernel::acpi::RootPointer> {
    system::with_config_table(|entries| {
        entries
            .iter()
            .find(|entry| entry.guid == ConfigTableEntry::ACPI2_GUID)
            .and_then(|entry| {
                acpi_root_pointer_from_entry(entry, kernel::acpi::RootPointerSource::UefiAcpi2)
            })
            .or_else(|| {
                entries
                    .iter()
                    .find(|entry| entry.guid == ConfigTableEntry::ACPI_GUID)
                    .and_then(|entry| {
                        acpi_root_pointer_from_entry(
                            entry,
                            kernel::acpi::RootPointerSource::UefiAcpi1,
                        )
                    })
            })
    })
}

fn acpi_root_pointer_from_entry(
    entry: &ConfigTableEntry,
    source: kernel::acpi::RootPointerSource,
) -> Option<kernel::acpi::RootPointer> {
    let physical_address = u64::try_from(entry.address.addr()).ok()?;
    (physical_address != 0).then_some(kernel::acpi::RootPointer::new(physical_address, source))
}

/// Return a compact framebuffer pixel-format label.
pub(crate) fn framebuffer_format_name(
    format: kernel::driver::display::framebuffer::ColorFormat,
) -> &'static str {
    match format {
        kernel::driver::display::framebuffer::ColorFormat::Rgb => "RGB",
        kernel::driver::display::framebuffer::ColorFormat::Bgr => "BGR",
    }
}

/// Import UEFI memory descriptors into the physical frame allocator.
pub(crate) fn import_boot_memory_map<'a>(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
    memory_descriptors: impl Iterator<Item = &'a MemoryDescriptor>,
) {
    for descriptor in memory_descriptors {
        if descriptor.ty == MemoryType::CONVENTIONAL {
            frame_allocator.add_region(
                kernel::memory::address::PhysAddr::new(descriptor.phys_start),
                descriptor.page_count,
            );
        } else {
            frame_allocator.reserve_region_for(
                kernel::memory::address::PhysAddr::new(descriptor.phys_start),
                descriptor.page_count,
                boot_memory_owner_for(descriptor.ty),
            );
        }
    }

    let owner_statistics = frame_allocator.owner_statistics();
    crate::log_info!(
        "memory",
        "Boot memory owner import: free={} firmware_reserved={} kernel_image={} mmio={}",
        owner_statistics.free,
        owner_statistics.firmware_reserved,
        owner_statistics.kernel_image,
        owner_statistics.mmio
    );
}

fn boot_memory_owner_for(
    memory_type: MemoryType,
) -> kernel::memory::frame_allocator::FrameRangeOwner {
    match memory_type {
        MemoryType::LOADER_CODE => kernel::memory::frame_allocator::FrameRangeOwner::KernelImage,
        MemoryType::MMIO | MemoryType::MMIO_PORT_SPACE => {
            kernel::memory::frame_allocator::FrameRangeOwner::Mmio
        }
        _ => kernel::memory::frame_allocator::FrameRangeOwner::FirmwareReserved,
    }
}

/// Return the active framebuffer byte size.
pub(crate) fn get_framebuffer_size(
    framebuffer_info: kernel::driver::display::framebuffer::FrameBufferInfo,
) -> u64 {
    (framebuffer_info.stride * framebuffer_info.vertical_resolution * 4) as u64
}

/// Allocate the framebuffer backbuffer from physical frames.
pub(crate) fn allocate_backbuffer(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
    framebuffer_size: u64,
) -> kernel::memory::address::KernelVirtualAddress {
    let backbuffer_pages = framebuffer_size.div_ceil(4096);
    let backbuffer_physical_range = frame_allocator
        .allocate_frames_for(
            backbuffer_pages,
            kernel::memory::frame_allocator::FrameRangeOwner::FramebufferBackbuffer,
        )
        .expect("OOM: failed to allocate framebuffer backbuffer");
    backbuffer_physical_range
        .start()
        .as_identity_mapped_kernel_address()
}
