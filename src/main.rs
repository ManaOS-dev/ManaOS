//! `ManaOS` kernel and UEFI entry point.

#![no_main]
#![no_std]
#![feature(abi_x86_interrupt)]
#![deny(missing_docs)]
#![deny(clippy::missing_safety_doc)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate)]

extern crate alloc;

mod arch;
mod kernel;
mod shared;

use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::system;
use uefi::table::cfg::ConfigTableEntry;
use uefi::{
    boot,
    mem::memory_map::{MemoryDescriptor, MemoryMap, MemoryType},
};

use crate::kernel::diagnostic::log::{LogField, LogLevel};
use crate::kernel::diagnostic::summary::{BootSummary, CheckStatus, FramebufferSummary};

const LOCAL_APIC_MMIO_MAPPING_SIZE: u64 = 4096;
const IOAPIC_MMIO_MAPPING_SIZE: u64 = 4096;
const LOCAL_APIC_TIMER_CALIBRATION_TICKS: u64 = 100;
const LOCAL_APIC_TIMER_POST_ACTIVATION_TICKS: u64 = 5;
const TIMER_SWITCH_SPIN_LIMIT: u64 = 10_000_000;

extern "C" fn idle_task() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

/// Panic handler: dump to serial and halt.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::serial_println!("[PANIC] {}", info);
    arch::hlt_loop();
}

/// Load a file from the EFI System Partition into memory (Boot Phase).
fn load_file(path: &str) -> &'static mut [u8] {
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

    // Allocate memory from UEFI pool
    let ptr = boot::allocate_pool(MemoryType::LOADER_DATA, size)
        .expect("Failed to allocate pool for file");

    // SAFETY: allocate_pool returned a valid pointer to a LOADER_DATA buffer of
    // exactly size bytes, and the buffer remains owned by the boot phase.
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr.as_ptr(), size) };
    file.read(buffer).expect("Failed to read file");

    buffer
}

fn get_framebuffer_info() -> kernel::driver::display::framebuffer::FrameBufferInfo {
    let graphics_output_handle = boot::get_handle_for_protocol::<GraphicsOutput>()
        .expect("GraphicsOutput handle is required for ManaOS framebuffer setup");
    let mut graphics_output =
        boot::open_protocol_exclusive::<GraphicsOutput>(graphics_output_handle)
            .expect("GraphicsOutput protocol is required for ManaOS framebuffer setup");
    kernel::driver::display::framebuffer::get_info(&mut graphics_output)
}

fn find_acpi_root_pointer() -> Option<kernel::acpi::RootPointer> {
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

fn framebuffer_format_name(
    format: kernel::driver::display::framebuffer::ColorFormat,
) -> &'static str {
    match format {
        kernel::driver::display::framebuffer::ColorFormat::Rgb => "RGB",
        kernel::driver::display::framebuffer::ColorFormat::Bgr => "BGR",
    }
}

fn import_boot_memory_map<'a>(
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

fn get_framebuffer_size(
    framebuffer_info: kernel::driver::display::framebuffer::FrameBufferInfo,
) -> u64 {
    (framebuffer_info.stride * framebuffer_info.vertical_resolution * 4) as u64
}

fn allocate_backbuffer(
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

fn initialize_scheduler(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    kernel::task::initialize();
    kernel::task::spawn(frame_allocator, idle_task);
    let task_id = kernel::task::get_current_task_id()
        .expect("scheduler must expose a bootstrap task after initialization");
    crate::log_info!("task", "Scheduler initialized. current_task={}", task_id);
}

fn verify_kernel_stack_guard_fault_diagnostics() {
    let diagnostic = kernel::task::get_kernel_stack_guard_fault_diagnostic_sample()
        .expect("kernel stack guard diagnostics must classify a scheduler-owned stack");
    crate::log_info!(
        "fault",
        "Kernel stack guard diagnostics verified: owner={} task={} guard={:#x} writable_start={:#x} stack_top={:#x}",
        diagnostic.owner().as_str(),
        diagnostic.task_identifier(),
        diagnostic.guard_page_start(),
        diagnostic.writable_start(),
        diagnostic.stack_top()
    );
}

fn initialize_architecture_and_drivers() {
    arch::init(kernel::interrupt::syscall_entry as *const () as u64);
    arch::x86_64::interrupt_descriptor_table::register_page_fault_reporter(
        kernel::interrupt::process_page_fault,
    );
    kernel::time::register_timer_ticks_provider(
        arch::x86_64::interrupt_descriptor_table::get_ticks,
    );
    kernel::profiler::register_timestamp_counter_provider(arch::x86_64::read_timestamp_counter);
    kernel::task::architecture::register_context_switch(arch::x86_64::switch_context);
    kernel::task::architecture::register_user_mode_switch(
        arch::x86_64::switch_to_user_mode_context,
    );
    kernel::task::architecture::register_returnable_user_mode_entry(
        arch::x86_64::enter_user_mode_once,
    );
    kernel::task::architecture::register_kernel_stack_installer(
        arch::x86_64::global_descriptor_table::set_privilege_stack_top,
    );
    kernel::task::user_mode::register_selectors(kernel::task::user_mode::UserModeSelectors {
        data: arch::x86_64::global_descriptor_table::USER_DATA_SELECTOR,
        code: arch::x86_64::global_descriptor_table::USER_CODE_SELECTOR,
    });
    crate::log_info!("arch", "Architecture initialized.");
    let user_selectors = kernel::task::user_mode::get_selectors();
    crate::log_info!(
        "task",
        "Ring 3 selectors installed. code={:#06x}, data={:#06x}",
        user_selectors.code,
        user_selectors.data
    );

    arch::x86_64::interrupt_descriptor_table::register_processors(
        arch::x86_64::interrupt_descriptor_table::InterruptProcessors {
            timer_tick: kernel::interrupt::process_timer_tick,
            keyboard_byte: kernel::interrupt::push_keyboard_byte,
            mouse_byte: kernel::interrupt::push_mouse_byte,
        },
    );

    crate::log_info!("driver", "Initializing mouse...");
    kernel::driver::input::mouse::init();
    crate::log_info!("driver", "Mouse initialized.");

    activate_ioapic_interrupt_routing();
    start_local_apic_timer_calibration();
    arch::x86_64::enable_interrupts();
    activate_local_apic_timer_ticks();
}

fn verify_kernel_filesystem() {
    crate::log_info!("fs", "Standard output is connected to /dev/console.");
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_OUTPUT, b"");
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_ERROR, b"");

    kernel::filesystem::mount_ram_file("/hello.txt", b"hello from ramfs\n");
    kernel::filesystem::mount_read_only_file("/docs/manual-smoke.txt", b"cat /disk/hello.txt\n");
    let descriptor =
        kernel::filesystem::open("/hello.txt").expect("ramfs smoke test file must open");
    let mut buffer = [0_u8; 32];
    kernel::filesystem::seek(descriptor, 0).expect("ramfs smoke test seek must succeed");
    let bytes_read =
        kernel::filesystem::read(descriptor, &mut buffer).expect("ramfs smoke test must read");
    kernel::filesystem::close(descriptor).expect("ramfs smoke test descriptor must close");
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_OUTPUT, &buffer[..bytes_read]);

    let dev_entries =
        kernel::filesystem::list_directory("/dev").expect("/dev listing must be available");
    crate::log_info!(
        "fs",
        "VFS directory listing smoke: path=/dev entries={}",
        dev_entries.len()
    );
    let dev_descriptor = kernel::filesystem::open("/dev").expect("/dev directory handle must open");
    let dev_metadata =
        kernel::filesystem::descriptor_metadata(dev_descriptor).expect("/dev stat must succeed");
    let mut directory_entry_count = 0_usize;
    while kernel::filesystem::read_directory(dev_descriptor)
        .expect("/dev readdir must succeed")
        .is_some()
    {
        directory_entry_count += 1;
    }
    kernel::filesystem::close(dev_descriptor).expect("/dev descriptor must close");
    crate::log_info!(
        "fs",
        "VFS directory handle smoke: path=/dev entries={} type={:?}",
        directory_entry_count,
        dev_metadata.file_type
    );

    let null_descriptor =
        kernel::filesystem::open("/dev/null").expect("null device must open during smoke test");
    let _ = kernel::filesystem::write(null_descriptor, b"discarded");
    kernel::filesystem::close(null_descriptor).expect("null descriptor must close");

    let _ = kernel::filesystem::read(kernel::filesystem::STANDARD_INPUT, &mut buffer);
}

fn verify_frame_allocator_rules() -> bool {
    let zero_skip_ok =
        kernel::memory::frame_allocator::verify_zero_address_skip_for_multi_frame_allocations();
    let range_tracking_ok =
        kernel::memory::frame_allocator::verify_reserved_used_and_free_range_tracking();
    let duplicate_allocation_ok =
        kernel::memory::frame_allocator::verify_duplicate_allocation_rejection();
    let contiguous_boundaries_ok =
        kernel::memory::frame_allocator::verify_contiguous_allocation_boundaries();
    let reserved_exclusion_ok = kernel::memory::frame_allocator::verify_reserved_range_exclusion();
    let owner_tracking_ok = kernel::memory::frame_allocator::verify_owner_tracking();
    let released_frame_reuse_ok = kernel::memory::frame_allocator::verify_released_frame_reuse();
    let owner_coverage_ok = kernel::memory::frame_allocator::verify_explicit_owner_coverage();
    let passed = zero_skip_ok
        && range_tracking_ok
        && duplicate_allocation_ok
        && contiguous_boundaries_ok
        && reserved_exclusion_ok
        && owner_tracking_ok
        && released_frame_reuse_ok
        && owner_coverage_ok;
    if passed {
        crate::log_info!(
            "memory",
            "Frame allocator self-checks passed: zero_skip=true range_tracking=true duplicate_allocation=true contiguous_boundaries=true reserved_exclusion=true owner_tracking=true released_frame_reuse=true owner_coverage=true"
        );
    } else {
        crate::log_error!(
            "memory",
            "Frame allocator self-checks failed: zero_skip={} range_tracking={} duplicate_allocation={} contiguous_boundaries={} reserved_exclusion={} owner_tracking={} released_frame_reuse={} owner_coverage={}",
            zero_skip_ok,
            range_tracking_ok,
            duplicate_allocation_ok,
            contiguous_boundaries_ok,
            reserved_exclusion_ok,
            owner_tracking_ok,
            released_frame_reuse_ok,
            owner_coverage_ok
        );
    }
    passed
}

fn verify_kernel_virtual_range_allocator_rules() {
    let non_overlapping_reuse_ok =
        kernel::memory::virtual_allocator::verify_kernel_virtual_range_allocation();
    let exhaustion_rejection_ok =
        kernel::memory::virtual_allocator::verify_kernel_virtual_range_exhaustion();

    if non_overlapping_reuse_ok && exhaustion_rejection_ok {
        crate::log_info!(
            "memory",
            "Kernel virtual range allocator self-checks passed: non_overlapping_reuse=true exhaustion_rejection=true"
        );
    } else {
        crate::log_error!(
            "memory",
            "Kernel virtual range allocator self-checks failed: non_overlapping_reuse={} exhaustion_rejection={}",
            non_overlapping_reuse_ok,
            exhaustion_rejection_ok
        );
    }
}

fn verify_dynamic_kernel_mapping_lifecycle(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    assert!(
        kernel::memory::paging::verify_kernel_dynamic_mapping_lifecycle(frame_allocator),
        "dynamic kernel mapping lifecycle smoke must pass"
    );
    crate::log_info!(
        "memory",
        "Dynamic kernel mapping lifecycle self-check passed: map=true unmap=true virtual_reuse=true physical_reuse=true"
    );
}

fn verify_user_address_space_template(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    assert!(
        kernel::memory::address_space::verify_user_address_space_template(
            frame_allocator,
            verify_kernel_filesystem as *const () as usize,
        ),
        "user address-space template smoke must pass"
    );
    crate::log_info!(
        "memory",
        "User address-space template self-check passed: kernel_shared=true user_window_empty=true"
    );
}

fn verify_user_address_space_reclaim(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    let reclaim = kernel::memory::address_space::verify_user_address_space_reclaim(frame_allocator)
        .expect("user address-space reclaim smoke must pass");
    crate::log_info!(
        "memory",
        "User address-space reclaim self-check passed: user_pages={} page_table_pages={}",
        reclaim.user_pages(),
        reclaim.page_table_pages()
    );
}

fn verify_elf_loader_rules() {
    assert!(
        kernel::elf::verify_invalid_elf_rejections(),
        "ELF invalid-image rejection smoke must pass"
    );
}

fn verify_acpi_parser_rules() -> bool {
    assert!(
        kernel::acpi::verify_parser_rules(),
        "ACPI parser self-check must pass"
    );
    crate::log_info!(
        "acpi",
        "ACPI parser self-check passed: rsdp=true root_table=true madt=true"
    );
    true
}

fn verify_acpi_root_table(
    root_pointer: Option<kernel::acpi::RootPointer>,
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) -> bool {
    let root_pointer =
        root_pointer.expect("UEFI ACPI RSDP configuration table is required before APIC setup");
    // SAFETY: The RSDP address came from the UEFI configuration table before
    // ExitBootServices, and paging has identity-mapped boot memory ranges.
    let diagnostics: kernel::acpi::Diagnostics =
        unsafe { kernel::acpi::inspect_root_pointer(root_pointer) }
            .expect("UEFI ACPI RSDP and root table must validate before APIC setup");
    let root_table: kernel::acpi::RootTableDiagnostics = diagnostics.root_table();
    log_acpi_root_table(&diagnostics, root_table);
    let madt: kernel::acpi::MadtDiagnostics = diagnostics.madt();
    log_acpi_madt(&madt);
    let topology: kernel::acpi::MadtInterruptTopology = madt.topology();
    let ioapic: kernel::acpi::MadtIoApic = topology
        .ioapic(0)
        .expect("ACPI MADT must contain at least one IOAPIC before IOAPIC setup");
    let local_apic: kernel::acpi::MadtLocalApic = topology
        .local_apic(0)
        .expect("ACPI MADT must contain at least one Local APIC before APIC setup");
    log_acpi_interrupt_topology(&topology, local_apic, ioapic);
    configure_apic_routing_provider(&madt, &topology, local_apic, ioapic, frame_allocator);
    true
}

fn log_acpi_root_table(
    diagnostics: &kernel::acpi::Diagnostics,
    root_table: kernel::acpi::RootTableDiagnostics,
) {
    let source = diagnostics.root_pointer().source().as_str();
    let revision = diagnostics.revision();
    let rsdt_address = diagnostics.rsdt_address();
    let root_table_kind: kernel::acpi::RootTableKind = root_table.kind();
    let root_table_label = root_table_kind.as_str();
    let root_address = root_table.physical_address();
    let root_revision = root_table.revision();
    let root_length = root_table.length();
    let root_entry_count = root_table.entry_count();
    if let Some(xsdt_address) = diagnostics.xsdt_address() {
        kernel::diagnostic::log::log_kv(
            LogLevel::Info,
            "acpi",
            format_args!("ACPI root table verified"),
            &[
                LogField::new("source", format_args!("{source}")),
                LogField::new("revision", format_args!("{revision}")),
                LogField::new("rsdt", format_args!("{rsdt_address:#x}")),
                LogField::new("xsdt", format_args!("{xsdt_address:#x}")),
                LogField::new("root_table", format_args!("{root_table_label}")),
                LogField::new("root_address", format_args!("{root_address:#x}")),
                LogField::new("root_revision", format_args!("{root_revision}")),
                LogField::new("root_length", format_args!("{root_length}")),
                LogField::new("entries", format_args!("{root_entry_count}")),
                LogField::new("checksum", format_args!("true")),
            ],
        );
    } else {
        kernel::diagnostic::log::log_kv(
            LogLevel::Info,
            "acpi",
            format_args!("ACPI root table verified"),
            &[
                LogField::new("source", format_args!("{source}")),
                LogField::new("revision", format_args!("{revision}")),
                LogField::new("rsdt", format_args!("{rsdt_address:#x}")),
                LogField::new("xsdt", format_args!("none")),
                LogField::new("root_table", format_args!("{root_table_label}")),
                LogField::new("root_address", format_args!("{root_address:#x}")),
                LogField::new("root_revision", format_args!("{root_revision}")),
                LogField::new("root_length", format_args!("{root_length}")),
                LogField::new("entries", format_args!("{root_entry_count}")),
                LogField::new("checksum", format_args!("true")),
            ],
        );
    }
}

fn log_acpi_madt(madt: &kernel::acpi::MadtDiagnostics) {
    let madt_address = madt.physical_address();
    let madt_revision = madt.revision();
    let madt_length = madt.length();
    let local_apic_address = madt.local_apic_address();
    let madt_flags = madt.flags();
    let pc_at_compatible = madt.pc_at_compatible();
    let madt_entries = madt.entry_count();
    let local_apics = madt.local_apic_count();
    let ioapics = madt.ioapic_count();
    let interrupt_source_overrides = madt.interrupt_source_override_count();
    let local_apic_nmis = madt.local_apic_nmi_count();
    let local_apic_address_overrides = madt.local_apic_address_override_count();
    let x2apics = madt.x2apic_count();
    kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "acpi",
        format_args!("ACPI MADT verified"),
        &[
            LogField::new("address", format_args!("{madt_address:#x}")),
            LogField::new("revision", format_args!("{madt_revision}")),
            LogField::new("length", format_args!("{madt_length}")),
            LogField::new("local_apic", format_args!("{local_apic_address:#x}")),
            LogField::new("flags", format_args!("{madt_flags:#x}")),
            LogField::new("pc_at_compatible", format_args!("{pc_at_compatible}")),
            LogField::new("entries", format_args!("{madt_entries}")),
            LogField::new("local_apics", format_args!("{local_apics}")),
            LogField::new("ioapics", format_args!("{ioapics}")),
            LogField::new(
                "interrupt_source_overrides",
                format_args!("{interrupt_source_overrides}"),
            ),
            LogField::new("local_apic_nmis", format_args!("{local_apic_nmis}")),
            LogField::new(
                "local_apic_address_overrides",
                format_args!("{local_apic_address_overrides}"),
            ),
            LogField::new("x2apics", format_args!("{x2apics}")),
            LogField::new("checksum", format_args!("true")),
        ],
    );
}

fn log_acpi_interrupt_topology(
    topology: &kernel::acpi::MadtInterruptTopology,
    local_apic: kernel::acpi::MadtLocalApic,
    ioapic: kernel::acpi::MadtIoApic,
) {
    let legacy_timer_override: Option<kernel::acpi::MadtInterruptSourceOverride> =
        topology.interrupt_source_override_for_legacy_irq(0);
    let local_apic_nmi: Option<kernel::acpi::MadtLocalApicNmi> = topology.local_apic_nmi(0);
    let x2apic: Option<kernel::acpi::MadtX2Apic> = topology.x2apic(0);
    let retained_local_apics = topology.retained_local_apic_count();
    let retained_ioapics = topology.retained_ioapic_count();
    let retained_interrupt_source_overrides = topology.retained_interrupt_source_override_count();
    let retained_local_apic_nmis = topology.retained_local_apic_nmi_count();
    let retained_x2apics = topology.retained_x2apic_count();
    let topology_truncated = topology.is_truncated();
    let local_apic0_processor = local_apic.processor_id();
    let local_apic0_id = local_apic.apic_id();
    let local_apic0_flags = local_apic.flags();
    let local_apic0_enabled = local_apic.is_enabled();
    let local_apic0_online_capable = local_apic.is_online_capable();
    let ioapic0_id = ioapic.id();
    let ioapic0_address = ioapic.physical_address();
    let ioapic0_gsi_base = ioapic.global_system_interrupt_base();
    let legacy_irq0_gsi = topology.global_system_interrupt_for_legacy_irq(0);
    let legacy_irq0_flags =
        legacy_timer_override.map_or(0, kernel::acpi::MadtInterruptSourceOverride::flags);
    let keyboard_legacy_gsi = topology.global_system_interrupt_for_legacy_irq(1);
    let mouse_legacy_gsi = topology.global_system_interrupt_for_legacy_irq(12);
    let local_apic_nmi0_lint = local_apic_nmi.map_or(0, kernel::acpi::MadtLocalApicNmi::lint);
    let x2apic0_present = x2apic.is_some();
    let x2apic0_id = x2apic.map_or(0, kernel::acpi::MadtX2Apic::x2apic_id);
    let x2apic_processor_uid = x2apic.map_or(0, kernel::acpi::MadtX2Apic::processor_uid);
    let x2apic0_flags = x2apic.map_or(0, kernel::acpi::MadtX2Apic::flags);
    let x2apic0_enabled = x2apic.is_some_and(kernel::acpi::MadtX2Apic::is_enabled);
    let x2apic0_online_capable = x2apic.is_some_and(kernel::acpi::MadtX2Apic::is_online_capable);
    kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "acpi",
        format_args!("ACPI interrupt topology verified"),
        &[
            LogField::new(
                "retained_local_apics",
                format_args!("{retained_local_apics}"),
            ),
            LogField::new("retained_ioapics", format_args!("{retained_ioapics}")),
            LogField::new(
                "retained_interrupt_source_overrides",
                format_args!("{retained_interrupt_source_overrides}"),
            ),
            LogField::new(
                "retained_local_apic_nmis",
                format_args!("{retained_local_apic_nmis}"),
            ),
            LogField::new("retained_x2apics", format_args!("{retained_x2apics}")),
            LogField::new("topology_truncated", format_args!("{topology_truncated}")),
            LogField::new(
                "local_apic0_processor",
                format_args!("{local_apic0_processor}"),
            ),
            LogField::new("local_apic0_id", format_args!("{local_apic0_id}")),
            LogField::new("local_apic0_flags", format_args!("{local_apic0_flags:#x}")),
            LogField::new("local_apic0_enabled", format_args!("{local_apic0_enabled}")),
            LogField::new(
                "local_apic0_online_capable",
                format_args!("{local_apic0_online_capable}"),
            ),
            LogField::new("ioapic0_id", format_args!("{ioapic0_id}")),
            LogField::new("ioapic0_address", format_args!("{ioapic0_address:#x}")),
            LogField::new("ioapic0_gsi_base", format_args!("{ioapic0_gsi_base}")),
            LogField::new("legacy_irq0_gsi", format_args!("{legacy_irq0_gsi}")),
            LogField::new("legacy_irq0_flags", format_args!("{legacy_irq0_flags:#x}")),
            LogField::new("legacy_irq1_gsi", format_args!("{keyboard_legacy_gsi}")),
            LogField::new("legacy_irq12_gsi", format_args!("{mouse_legacy_gsi}")),
            LogField::new(
                "local_apic_nmi0_lint",
                format_args!("{local_apic_nmi0_lint}"),
            ),
            LogField::new("x2apic0_present", format_args!("{x2apic0_present}")),
            LogField::new("x2apic0_id", format_args!("{x2apic0_id}")),
            LogField::new("x2apic0_uid", format_args!("{x2apic_processor_uid}")),
            LogField::new("x2apic0_flags", format_args!("{x2apic0_flags:#x}")),
            LogField::new("x2apic0_enabled", format_args!("{x2apic0_enabled}")),
            LogField::new(
                "x2apic0_online_capable",
                format_args!("{x2apic0_online_capable}"),
            ),
        ],
    );
}

fn configure_apic_routing_provider(
    madt: &kernel::acpi::MadtDiagnostics,
    topology: &kernel::acpi::MadtInterruptTopology,
    local_apic: kernel::acpi::MadtLocalApic,
    ioapic: kernel::acpi::MadtIoApic,
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    let local_apic_configuration = arch::x86_64::interrupt_controller::LocalApicConfiguration::new(
        madt.local_apic_address(),
        u32::from(local_apic.apic_id()),
        local_apic.is_enabled(),
        local_apic.is_online_capable(),
    );
    let ioapic_configuration = arch::x86_64::interrupt_controller::IoApicConfiguration::new(
        ioapic.id(),
        ioapic.physical_address(),
        ioapic.global_system_interrupt_base(),
    );
    let mut routing_configuration =
        arch::x86_64::interrupt_controller::ApicRoutingConfiguration::new(
            local_apic_configuration,
            ioapic_configuration,
        );

    let mut source_override_index = 0;
    while source_override_index < topology.retained_interrupt_source_override_count() {
        if let Some(source_override) = topology.interrupt_source_override(source_override_index) {
            if source_override.bus() == 0 {
                routing_configuration.push_legacy_irq_route(
                    arch::x86_64::interrupt_controller::LegacyIrqRoute::new(
                        source_override.source_irq(),
                        source_override.global_system_interrupt(),
                        source_override.flags(),
                    ),
                );
            }
        }
        source_override_index += 1;
    }

    arch::x86_64::interrupt_controller::configure_apic_routing_provider(&routing_configuration);
    let status = arch::x86_64::interrupt_controller::get_apic_routing_provider_status();
    log_apic_routing_provider_status(status);
    log_ioapic_redirection_plan(status);
    verify_local_apic_eoi_provider(frame_allocator, status.local_apic());
    stage_ioapic_redirection_entries(frame_allocator, status.ioapic());
}

fn log_apic_routing_provider_status(
    status: arch::x86_64::interrupt_controller::ApicRoutingProviderStatus,
) {
    let configured_local_apic = status.local_apic();
    let configured_ioapic = status.ioapic();
    crate::log_info!(
        "arch",
        "APIC routing provider configured: configured={} routing_active={} local_apic_supported={} local_apic_address={:#x} local_apic_id={} local_apic_enabled={} local_apic_online_capable={} ioapic_id={} ioapic_address={:#x} ioapic_gsi_base={} legacy_irq_routes={} legacy_irq0_gsi={} legacy_irq0_flags={:#x} legacy_irq1_gsi={} legacy_irq12_gsi={} route_truncated={}",
        status.is_configured(),
        status.is_routing_active(),
        status.has_local_apic_support(),
        configured_local_apic.physical_address(),
        configured_local_apic.apic_id(),
        configured_local_apic.is_enabled(),
        configured_local_apic.is_online_capable(),
        configured_ioapic.id(),
        configured_ioapic.physical_address(),
        configured_ioapic.global_system_interrupt_base(),
        status.legacy_irq_route_count(),
        status.legacy_irq0_global_system_interrupt(),
        status.legacy_irq0_flags(),
        status.legacy_irq1_global_system_interrupt(),
        status.legacy_irq12_global_system_interrupt(),
        status.is_truncated()
    );
}

fn log_ioapic_redirection_plan(
    status: arch::x86_64::interrupt_controller::ApicRoutingProviderStatus,
) {
    let redirection_plan = status.redirection_plan();
    let timer_redirection_entry = redirection_plan
        .entry_for_legacy_irq(0)
        .expect("APIC routing provider must plan a timer redirection entry");
    let keyboard_redirection_entry = redirection_plan
        .entry_for_legacy_irq(1)
        .expect("APIC routing provider must plan a keyboard redirection entry");
    let mouse_redirection_entry = redirection_plan
        .entry_for_legacy_irq(12)
        .expect("APIC routing provider must plan a mouse redirection entry");
    let first_redirection_entry = redirection_plan
        .entry(0)
        .expect("APIC routing provider must retain the first redirection entry");
    crate::log_info!(
        "arch",
        "IOAPIC redirection plan verified: entries={} truncated={} routing_active={} first_irq={} timer_irq={} timer_gsi={} timer_vector={} timer_table_index={} timer_low_register={:#x} timer_high_register={:#x} timer_low_value={:#x} timer_high_value={:#x} timer_active_low={} timer_level_triggered={} timer_masked={} keyboard_irq={} keyboard_gsi={} keyboard_vector={} keyboard_table_index={} keyboard_low_register={:#x} mouse_irq={} mouse_gsi={} mouse_vector={} mouse_table_index={} mouse_low_register={:#x}",
        redirection_plan.entry_count(),
        redirection_plan.is_truncated(),
        status.is_routing_active(),
        first_redirection_entry.legacy_irq(),
        timer_redirection_entry.legacy_irq(),
        timer_redirection_entry.global_system_interrupt(),
        timer_redirection_entry.vector(),
        timer_redirection_entry.table_index(),
        timer_redirection_entry.low_register(),
        timer_redirection_entry.high_register(),
        timer_redirection_entry.low_value(),
        timer_redirection_entry.high_value(),
        timer_redirection_entry.is_active_low(),
        timer_redirection_entry.is_level_triggered(),
        timer_redirection_entry.is_masked(),
        keyboard_redirection_entry.legacy_irq(),
        keyboard_redirection_entry.global_system_interrupt(),
        keyboard_redirection_entry.vector(),
        keyboard_redirection_entry.table_index(),
        keyboard_redirection_entry.low_register(),
        mouse_redirection_entry.legacy_irq(),
        mouse_redirection_entry.global_system_interrupt(),
        mouse_redirection_entry.vector(),
        mouse_redirection_entry.table_index(),
        mouse_redirection_entry.low_register()
    );
}

fn verify_local_apic_eoi_provider(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
    configured_local_apic: arch::x86_64::interrupt_controller::LocalApicConfiguration,
) {
    // SAFETY: The MADT Local APIC address describes an MMIO register page, and
    // this boot-time mapping keeps it identity-mapped before arch-owned Local
    // APIC register access reads it for EOI-provider diagnostics.
    unsafe {
        kernel::memory::paging::map_kernel_mmio_range(
            frame_allocator,
            kernel::memory::address::PhysAddr::new(configured_local_apic.physical_address()),
            LOCAL_APIC_MMIO_MAPPING_SIZE,
        );
    }
    crate::log_info!(
        "arch",
        "Local APIC MMIO mapped: address={:#x} size={}",
        configured_local_apic.physical_address(),
        LOCAL_APIC_MMIO_MAPPING_SIZE
    );
    // SAFETY: The Local APIC MMIO page was just identity-mapped for boot-time
    // diagnostics, and this read does not enable APIC interrupt routing.
    let local_apic_eoi_status =
        unsafe { arch::x86_64::interrupt_controller::inspect_local_apic_eoi_provider() }
            .expect("Local APIC EOI provider must be configured before inspection");
    crate::log_info!(
        "arch",
        "Local APIC EOI provider verified: configured={} routing_active={} software_enabled={} local_apic_address={:#x} local_apic_id={} version={:#x} max_lvt_entry={} spurious_vector={:#x}",
        local_apic_eoi_status.is_configured(),
        local_apic_eoi_status.is_routing_active(),
        local_apic_eoi_status.is_software_enabled(),
        local_apic_eoi_status.physical_address(),
        local_apic_eoi_status.apic_id(),
        local_apic_eoi_status.version(),
        local_apic_eoi_status.maximum_lvt_entry(),
        local_apic_eoi_status.spurious_interrupt_vector()
    );
}

fn stage_ioapic_redirection_entries(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
    configured_ioapic: arch::x86_64::interrupt_controller::IoApicConfiguration,
) {
    // SAFETY: The MADT IOAPIC address describes an MMIO register page, and this
    // boot-time mapping keeps it identity-mapped as writable uncached memory
    // before arch-owned IOAPIC register access reads or writes it.
    unsafe {
        kernel::memory::paging::map_kernel_mmio_range(
            frame_allocator,
            kernel::memory::address::PhysAddr::new(configured_ioapic.physical_address()),
            IOAPIC_MMIO_MAPPING_SIZE,
        );
    }
    crate::log_info!(
        "arch",
        "IOAPIC MMIO mapped: address={:#x} size={}",
        configured_ioapic.physical_address(),
        IOAPIC_MMIO_MAPPING_SIZE
    );
    // SAFETY: The IOAPIC MMIO page was just identity-mapped for boot-time
    // access, and routing remains disabled because only masked entries are
    // staged before APIC EOI handling is available.
    let staging_status =
        unsafe { arch::x86_64::interrupt_controller::stage_masked_ioapic_redirection_entries() }
            .expect("APIC routing provider must be configured before IOAPIC staging");
    crate::log_info!(
        "arch",
        "IOAPIC redirection staging verified: entries={} staged={} readback_matches={} routing_active={} masked={} ioapic_version={:#x} max_redirection_entry={} out_of_range_entries={} timer_low_readback={:#x} timer_high_readback={:#x} keyboard_low_readback={:#x} keyboard_high_readback={:#x} mouse_low_readback={:#x} mouse_high_readback={:#x}",
        staging_status.planned_entry_count(),
        staging_status.staged_entry_count(),
        staging_status.readback_matches(),
        arch::x86_64::interrupt_controller::has_ioapic_routing(),
        staging_status.all_entries_masked(),
        staging_status.version(),
        staging_status.maximum_redirection_entry(),
        staging_status.out_of_range_entry_count(),
        staging_status.timer_low_readback(),
        staging_status.timer_high_readback(),
        staging_status.keyboard_low_readback(),
        staging_status.keyboard_high_readback(),
        staging_status.mouse_low_readback(),
        staging_status.mouse_high_readback()
    );
}

fn activate_ioapic_interrupt_routing() {
    // SAFETY: Architecture initialization keeps CPU interrupts disabled here,
    // and Local APIC plus IOAPIC MMIO pages were identity-mapped during ACPI
    // verification before this activation step.
    let activation_status =
        unsafe { arch::x86_64::interrupt_controller::activate_ioapic_routing() }
            .expect("APIC routing provider must be configured before IOAPIC activation");
    let eoi_status = arch::x86_64::interrupt_controller::get_end_of_interrupt_status();
    crate::log_info!(
        "arch",
        "IOAPIC routing activated: entries={} activated={} readback_matches={} routing_active={} masked={} local_apic_software_enabled={} legacy_pic_masked={} out_of_range_entries={} timer_low_readback={:#x} timer_high_readback={:#x} keyboard_low_readback={:#x} keyboard_high_readback={:#x} mouse_low_readback={:#x} mouse_high_readback={:#x} apic_eoi_count={} legacy_eoi_count={}",
        activation_status.planned_entry_count(),
        activation_status.activated_entry_count(),
        activation_status.readback_matches(),
        activation_status.is_routing_active(),
        !activation_status.all_entries_unmasked(),
        activation_status.local_apic_software_enabled(),
        activation_status.legacy_pic_masked(),
        activation_status.out_of_range_entry_count(),
        activation_status.timer_low_readback(),
        activation_status.timer_high_readback(),
        activation_status.keyboard_low_readback(),
        activation_status.keyboard_high_readback(),
        activation_status.mouse_low_readback(),
        activation_status.mouse_high_readback(),
        eoi_status.apic_count(),
        eoi_status.legacy_count()
    );
}

fn verify_apic_eoi_diagnostics() -> arch::x86_64::interrupt_controller::EndOfInterruptStatus {
    let eoi_status = arch::x86_64::interrupt_controller::get_end_of_interrupt_status();
    assert!(
        eoi_status.is_ioapic_routing_active(),
        "IOAPIC routing must be active before APIC EOI diagnostics"
    );
    assert!(
        eoi_status.apic_count() > 0,
        "APIC EOI count must increase after timer interrupts"
    );
    assert_eq!(
        eoi_status.legacy_count(),
        0,
        "legacy PIC EOI count must stay zero after IOAPIC activation"
    );
    crate::log_info!(
        "arch",
        "APIC EOI diagnostics verified: routing_active={} apic_eoi_count={} legacy_eoi_count={}",
        eoi_status.is_ioapic_routing_active(),
        eoi_status.apic_count(),
        eoi_status.legacy_count()
    );
    eoi_status
}

fn verify_interrupt_vector_diagnostics() -> bool {
    // SAFETY: The Local APIC MMIO page remains identity-mapped after boot-time
    // APIC setup and can be read for diagnostic verification.
    let local_apic_eoi_status =
        unsafe { arch::x86_64::interrupt_controller::inspect_local_apic_eoi_provider() }
            .expect("Local APIC EOI provider must be configured before vector diagnostics");
    let vector_diagnostics =
        arch::x86_64::interrupt_descriptor_table::get_interrupt_vector_diagnostics();
    assert!(
        local_apic_eoi_status.is_software_enabled(),
        "Local APIC must be software-enabled before vector diagnostics"
    );
    assert!(
        local_apic_eoi_status.has_diagnostic_spurious_interrupt_vector(),
        "Local APIC spurious interrupt vector must use the diagnostic vector"
    );
    assert_eq!(
        vector_diagnostics.spurious_interrupt_vector(),
        local_apic_eoi_status.spurious_interrupt_vector_number(),
        "IDT and Local APIC spurious interrupt vectors must match"
    );
    assert_eq!(
        vector_diagnostics.spurious_interrupt_count(),
        0,
        "boot smoke should not observe Local APIC spurious interrupts"
    );
    assert_eq!(
        vector_diagnostics.unexpected_external_interrupt_count(),
        0,
        "boot smoke should not observe unexpected external interrupts"
    );
    crate::log_info!(
        "arch",
        "Interrupt vector diagnostics verified: spurious_vector={} spurious_count={} unexpected_external_count={}",
        vector_diagnostics.spurious_interrupt_vector(),
        vector_diagnostics.spurious_interrupt_count(),
        vector_diagnostics.unexpected_external_interrupt_count()
    );
    true
}

fn start_local_apic_timer_calibration() {
    let start_ticks = kernel::time::get_timer_ticks();
    // SAFETY: The Local APIC MMIO page was identity-mapped during ACPI
    // verification, and the timer remains masked during this calibration
    // sample.
    let status = unsafe {
        arch::x86_64::interval_timer::start_masked_local_apic_timer_calibration(start_ticks)
    }
    .expect("Local APIC timer calibration requires APIC provider data");
    crate::log_info!(
        "arch",
        "Local APIC timer calibration started: configured={} armed={} masked={} address={:#x} vector={} divide={} lvt_timer={:#x} divide_config={:#x} initial_count={} current_count={} start_ticks={}",
        status.is_configured(),
        status.is_armed(),
        status.is_masked(),
        status.physical_address(),
        status.vector(),
        status.divide_denominator(),
        status.lvt_timer(),
        status.divide_configuration(),
        status.initial_count(),
        status.current_count(),
        status.start_ticks()
    );
}

fn activate_local_apic_timer_ticks() {
    let calibration_ticks = wait_for_timer_ticks(LOCAL_APIC_TIMER_CALIBRATION_TICKS);
    let calibration_status = verify_local_apic_timer_calibration(calibration_ticks);
    arch::x86_64::disable_interrupts();

    // SAFETY: Interrupts are disabled during timer-source switching. IOAPIC
    // routing is active, and the IOAPIC MMIO page was identity-mapped during
    // ACPI verification.
    let timer_route_status = unsafe {
        arch::x86_64::interrupt_controller::mask_ioapic_timer_route_for_local_apic_timer()
    }
    .expect("IOAPIC timer route must be available before Local APIC timer activation");
    assert!(
        timer_route_status.readback_matches() && timer_route_status.is_masked(),
        "IOAPIC timer route must be masked before Local APIC timer activation"
    );
    assert!(
        timer_route_status.is_routing_active(),
        "IOAPIC routing must remain active for keyboard and mouse routes"
    );
    crate::log_info!(
        "arch",
        "IOAPIC timer route masked for Local APIC timer: routing_active={} readback_matches={} masked={} timer_gsi={} table_index={} low_register={:#x} high_register={:#x} low_readback={:#x} high_readback={:#x}",
        timer_route_status.is_routing_active(),
        timer_route_status.readback_matches(),
        timer_route_status.is_masked(),
        timer_route_status.global_system_interrupt(),
        timer_route_status.table_index(),
        timer_route_status.low_register(),
        timer_route_status.high_register(),
        timer_route_status.low_readback(),
        timer_route_status.high_readback()
    );

    // SAFETY: Interrupts are disabled during timer-source switching, and the
    // Local APIC MMIO page remains identity-mapped after ACPI verification.
    let active_status = unsafe {
        arch::x86_64::interval_timer::activate_local_apic_timer_from_calibration(
            calibration_status,
            calibration_ticks,
        )
    };
    assert!(
        active_status.is_configured()
            && active_status.is_running()
            && active_status.is_periodic()
            && !active_status.is_masked(),
        "Local APIC timer must be active, periodic, and unmasked"
    );
    crate::log_info!(
        "arch",
        "Local APIC timer activated: configured={} running={} masked={} periodic={} address={:#x} vector={} divide={} activation_ticks={} current_ticks={} initial_count={} current_count={} calibration_counts_per_tick={} lvt_timer={:#x} divide_config={:#x}",
        active_status.is_configured(),
        active_status.is_running(),
        active_status.is_masked(),
        active_status.is_periodic(),
        active_status.physical_address(),
        active_status.vector(),
        active_status.divide_denominator(),
        active_status.activation_ticks(),
        active_status.current_ticks(),
        active_status.initial_count(),
        active_status.current_count(),
        active_status.calibration_counts_per_tick(),
        active_status.lvt_timer(),
        active_status.divide_configuration()
    );

    arch::x86_64::enable_interrupts();
    verify_local_apic_timer_tick_source(LOCAL_APIC_TIMER_POST_ACTIVATION_TICKS);
}

fn wait_for_timer_ticks(required_ticks: u64) -> u64 {
    let start_ticks = kernel::time::get_timer_ticks();
    let target_ticks = start_ticks
        .checked_add(required_ticks)
        .expect("timer tick wait target overflowed");
    let mut spin_count = 0;
    while kernel::time::get_timer_ticks() < target_ticks && spin_count < TIMER_SWITCH_SPIN_LIMIT {
        x86_64::instructions::hlt();
        spin_count += 1;
    }
    let current_ticks = kernel::time::get_timer_ticks();
    assert!(
        current_ticks >= target_ticks,
        "timer ticks did not advance enough during backend switching"
    );
    current_ticks
}

fn verify_local_apic_timer_calibration(
    current_ticks: u64,
) -> arch::x86_64::interval_timer::LocalApicTimerCalibrationStatus {
    // SAFETY: The Local APIC MMIO page remains identity-mapped for the kernel
    // after boot-time APIC setup.
    let status = unsafe {
        arch::x86_64::interval_timer::inspect_masked_local_apic_timer_calibration(current_ticks)
    }
    .expect("Local APIC timer calibration sample must be armed before verification");
    assert!(
        status.is_configured() && status.is_armed(),
        "Local APIC timer calibration sample must stay configured and armed"
    );
    assert!(
        status.is_masked(),
        "Local APIC timer calibration must not unmask the timer interrupt"
    );
    assert!(
        status.elapsed_ticks() > 0,
        "PIT ticks must advance before Local APIC timer calibration verification"
    );
    assert!(
        status.has_decremented(),
        "Local APIC timer current count must decrease during calibration"
    );
    assert!(
        !status.has_expired(),
        "Local APIC timer calibration sample must not expire before verification"
    );
    assert!(
        status.counts_per_tick() > 0,
        "Local APIC timer calibration must observe counts per PIT tick"
    );
    crate::log_info!(
        "arch",
        "Local APIC timer calibration verified: configured={} armed={} masked={} decremented={} expired={} address={:#x} vector={} divide={} start_ticks={} current_ticks={} elapsed_ticks={} initial_count={} current_count={} elapsed_counts={} counts_per_tick={} lvt_timer={:#x} divide_config={:#x}",
        status.is_configured(),
        status.is_armed(),
        status.is_masked(),
        status.has_decremented(),
        status.has_expired(),
        status.physical_address(),
        status.vector(),
        status.divide_denominator(),
        status.start_ticks(),
        status.current_ticks(),
        status.elapsed_ticks(),
        status.initial_count(),
        status.current_count(),
        status.elapsed_counts(),
        status.counts_per_tick(),
        status.lvt_timer(),
        status.divide_configuration()
    );
    status
}

fn verify_local_apic_timer_tick_source(required_ticks: u64) {
    let current_ticks = wait_for_timer_ticks(required_ticks);
    let status = verify_active_local_apic_timer(current_ticks);
    log_active_local_apic_timer_status("Local APIC timer tick source verified", status);
}

fn verify_local_apic_timer_post_smoke() -> bool {
    let status = verify_active_local_apic_timer(kernel::time::get_timer_ticks());
    log_active_local_apic_timer_status("Local APIC timer post-smoke verified", status);
    true
}

fn verify_active_local_apic_timer(
    current_ticks: u64,
) -> arch::x86_64::interval_timer::LocalApicTimerActiveStatus {
    // SAFETY: The Local APIC MMIO page remains identity-mapped for the kernel
    // after boot-time APIC setup.
    let status =
        unsafe { arch::x86_64::interval_timer::inspect_active_local_apic_timer(current_ticks) }
            .expect("Local APIC timer must be active before inspection");
    assert!(
        status.is_configured()
            && status.is_running()
            && status.is_periodic()
            && !status.is_masked(),
        "Local APIC timer must remain active, periodic, and unmasked"
    );
    assert!(
        status.elapsed_ticks() > 0,
        "Local APIC timer must advance scheduler ticks after activation"
    );
    status
}

fn log_active_local_apic_timer_status(
    message: &str,
    status: arch::x86_64::interval_timer::LocalApicTimerActiveStatus,
) {
    crate::log_info!(
        "arch",
        "{}: configured={} running={} masked={} periodic={} address={:#x} vector={} divide={} activation_ticks={} current_ticks={} elapsed_ticks={} initial_count={} current_count={} calibration_counts_per_tick={} lvt_timer={:#x} divide_config={:#x}",
        message,
        status.is_configured(),
        status.is_running(),
        status.is_masked(),
        status.is_periodic(),
        status.physical_address(),
        status.vector(),
        status.divide_denominator(),
        status.activation_ticks(),
        status.current_ticks(),
        status.elapsed_ticks(),
        status.initial_count(),
        status.current_count(),
        status.calibration_counts_per_tick(),
        status.lvt_timer(),
        status.divide_configuration()
    );
}

fn verify_mounted_disk_file(path: &str) {
    let descriptor = kernel::filesystem::open(path).expect("mounted disk file must open");
    let mut buffer = [0_u8; 64];
    let bytes_read =
        kernel::filesystem::read(descriptor, &mut buffer).expect("mounted disk file must read");
    kernel::filesystem::close(descriptor).expect("mounted disk file descriptor must close");
    crate::log_info!(
        "fs",
        "Disk file smoke read: path={} bytes={}",
        path,
        bytes_read
    );
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_OUTPUT, &buffer[..bytes_read]);
}

fn verify_kernel_console_pipeline() -> bool {
    const PIPELINE_COMMAND: &str = "cat /disk/hello.txt | grep FAT32";

    match kernel::console::verify_pipeline_smoke(PIPELINE_COMMAND) {
        Some(output_lines) if output_lines > 0 => {
            crate::log_info!(
                "console",
                "Pipeline command smoke passed: command=\"{}\" output_lines={}",
                PIPELINE_COMMAND,
                output_lines
            );
            true
        }
        _ => {
            crate::log_warn!(
                "console",
                "Pipeline command smoke failed: command=\"{}\"",
                PIPELINE_COMMAND
            );
            false
        }
    }
}

fn verify_primary_storage_device() {
    let Some(data_address) = kernel::driver::storage::get_primary_data_address() else {
        return;
    };

    if kernel::driver::storage::read_primary_blocks(0, 2, data_address) {
        crate::log_info!(
            "storage",
            "Primary block device multi-sector read smoke passed."
        );
    } else {
        crate::log_warn!(
            "storage",
            "Primary block device multi-sector read smoke failed."
        );
    }
}

fn mount_detected_disk_files() -> bool {
    let mut hello_mounted = false;
    for file in kernel::driver::storage::get_detected_files() {
        kernel::filesystem::mount_fat32_file(
            &file.mount_path,
            file.size,
            file.backend_index,
            kernel::driver::storage::read_detected_file_range,
        );
        crate::log_info!(
            "fs",
            "Mounted disk file: path={} bytes={}",
            file.mount_path,
            file.size
        );
        if file.mount_path == "/disk/hello.txt" {
            hello_mounted = true;
        }
    }
    hello_mounted
}

fn record_storage_boot_summary(boot_summary: &mut BootSummary) {
    let storage_devices = kernel::driver::storage::get_storage_devices();
    let selected_partition = kernel::driver::storage::get_selected_partition();
    let detected_files = kernel::driver::storage::get_detected_files();
    boot_summary.ahci = CheckStatus::from_bool(!storage_devices.is_empty());
    boot_summary.gpt = CheckStatus::from_bool(selected_partition.is_some());
    boot_summary.fat32 = CheckStatus::from_bool(!detected_files.is_empty());
    boot_summary.mounted_files = Some(detected_files.len());

    let mut filesystem_smoke_passed = false;
    if mount_detected_disk_files() {
        verify_mounted_disk_file("/disk/hello.txt");
        filesystem_smoke_passed = verify_kernel_console_pipeline();
    }
    boot_summary.smoke_tests.filesystem = CheckStatus::from_bool(filesystem_smoke_passed);
}

fn run_post_userspace_diagnostics(
    boot_summary: &mut BootSummary,
    frame_allocator: &kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    kernel::diagnostic::log::section("Diagnostics / Smoke Tests");
    let eoi_status = verify_apic_eoi_diagnostics();
    boot_summary.ioapic_active = Some(eoi_status.is_ioapic_routing_active());
    boot_summary.apic_eoi_count = Some(eoi_status.apic_count());
    boot_summary.legacy_eoi_count = Some(eoi_status.legacy_count());
    boot_summary.local_apic_enabled = Some(verify_interrupt_vector_diagnostics());
    if verify_local_apic_timer_post_smoke() {
        boot_summary.timer = Some("Local APIC periodic");
    }

    kernel::diagnostic::smoke::verify_scheduler_task_diagnostics(2);
    kernel::diagnostic::smoke::verify_scheduler_task_snapshots(2);
    record_scheduler_boot_summary(boot_summary);
    kernel::diagnostic::smoke::record_memory_diagnostics_snapshot(frame_allocator);
    boot_summary.free_frames = Some(frame_allocator.statistics().free);
    record_console_smoke_summary(boot_summary);
    boot_summary.emit();
}

fn record_scheduler_boot_summary(boot_summary: &mut BootSummary) {
    if let Some(diagnostics) = kernel::task::get_scheduler_diagnostics() {
        boot_summary.user_tasks_spawned = Some(diagnostics.user_tasks());
        boot_summary.user_tasks_exited = Some(diagnostics.finished_tasks());
        boot_summary.user_resources_freed = CheckStatus::from_bool(
            diagnostics.reclaimed_user_resource_records() == diagnostics.user_tasks()
                && diagnostics.reclaimed_user_address_spaces() == diagnostics.user_tasks()
                && diagnostics.reclaimed_user_kernel_stacks() == diagnostics.user_tasks()
                && diagnostics.active_user_address_spaces() == 0,
        );
        boot_summary.smoke_tests.scheduler = CheckStatus::Pass;
        boot_summary.smoke_tests.preemption = CheckStatus::from_bool(
            diagnostics.preemption_enabled() && diagnostics.timer_preemptions() > 0,
        );
    }
}

fn record_console_smoke_summary(boot_summary: &mut BootSummary) {
    let scheduler_console_passed = kernel::diagnostic::smoke::verify_scheduler_console_command();
    let memory_console_passed = kernel::diagnostic::smoke::verify_memory_console_command();
    let syscall_console_passed = kernel::diagnostic::smoke::verify_syscall_trace_console_command();
    let status_strip_passed = kernel::diagnostic::smoke::verify_console_status_strip();
    boot_summary.smoke_tests.scheduler = CheckStatus::from_bool(
        boot_summary.smoke_tests.scheduler == CheckStatus::Pass
            && scheduler_console_passed
            && memory_console_passed
            && status_strip_passed,
    );
    boot_summary.smoke_tests.syscall = CheckStatus::from_bool(syscall_console_passed);
}

#[entry]
fn main() -> Status {
    // ────────────────────────────────────────────────
    // Boot Phase (UEFI Services available)
    // ────────────────────────────────────────────────
    kernel::logger::init();

    log::info!("ManaOS booting (HAL edition)...");

    let mut boot_summary = BootSummary::new();
    let framebuffer_info = get_framebuffer_info();
    boot_summary.framebuffer = Some(FramebufferSummary {
        width: framebuffer_info.horizontal_resolution,
        height: framebuffer_info.vertical_resolution,
        stride: framebuffer_info.stride,
        format: framebuffer_format_name(framebuffer_info.format),
    });

    log::info!("Calling ExitBootServices...");

    // Pre-load fonts before exiting boot services
    let font_inter = load_file("Inter.ttf");
    let font_noto = load_file("NotoSansJP.ttf");
    let acpi_root_pointer = find_acpi_root_pointer();

    // ────────────────────────────────────────────────
    // ExitBootServices
    // ────────────────────────────────────────────────
    kernel::logger::disable();
    // SAFETY: All required boot service resources have been acquired and UEFI
    // console logging is disabled before leaving the boot-services phase.
    let mmap = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };

    // ────────────────────────────────────────────────
    // Kernel Phase
    // ────────────────────────────────────────────────
    kernel::serial::init();
    kernel::diagnostic::log::section("Boot Services / UEFI");
    crate::log_info!("serial", "ExitBootServices OK.");
    boot_summary.exit_boot_services = CheckStatus::Pass;

    kernel::diagnostic::log::section("Memory");
    let mut frame_allocator = kernel::memory::frame_allocator::PhysicalFrameAllocator::new();
    import_boot_memory_map(&mut frame_allocator, mmap.entries());
    boot_summary.frame_allocator = CheckStatus::from_bool(verify_frame_allocator_rules());
    verify_kernel_virtual_range_allocator_rules();
    verify_elf_loader_rules();

    // ────────────────────────────────────────────────
    // Kernel Phase (UEFI Services unavailable)
    // ────────────────────────────────────────────────
    crate::log_info!("kernel", "ManaOS Kernel phase started.");

    kernel::diagnostic::log::section("Paging");
    let framebuffer_size = get_framebuffer_size(framebuffer_info);
    let backbuffer_address = allocate_backbuffer(&mut frame_allocator, framebuffer_size);

    kernel::boot::initialize(
        &mut frame_allocator,
        mmap.entries(),
        framebuffer_info,
        kernel::driver::display::font::FontAssets {
            inter: font_inter,
            noto: font_noto,
        },
        backbuffer_address,
    );
    boot_summary.kernel_heap_mib = Some(kernel::memory::heap::HEAP_SIZE / (1024 * 1024));
    verify_dynamic_kernel_mapping_lifecycle(&mut frame_allocator);
    verify_user_address_space_template(&mut frame_allocator);
    verify_user_address_space_reclaim(&mut frame_allocator);

    kernel::diagnostic::log::section("ACPI");
    let acpi_parser_passed = verify_acpi_parser_rules();
    let acpi_root_passed = verify_acpi_root_table(acpi_root_pointer, &mut frame_allocator);
    boot_summary.acpi = CheckStatus::from_bool(acpi_parser_passed && acpi_root_passed);

    kernel::diagnostic::log::section("Filesystem");
    kernel::filesystem::initialize();
    crate::log_info!("fs", "Kernel filesystem initialized.");
    verify_kernel_filesystem();

    kernel::diagnostic::log::section("Storage");
    kernel::driver::storage::init(
        &mut frame_allocator,
        kernel::driver::storage::PciConfigurationAccess::new(
            arch::x86_64::pci_configuration::read_config32,
            arch::x86_64::pci_configuration::write_config32,
        ),
    );
    verify_primary_storage_device();
    record_storage_boot_summary(&mut boot_summary);

    kernel::diagnostic::log::section("Scheduler");
    initialize_scheduler(&mut frame_allocator);
    verify_kernel_stack_guard_fault_diagnostics();

    kernel::diagnostic::log::section("Interrupts");
    initialize_architecture_and_drivers();

    crate::log_info!("kernel", "ManaOS Kernel is alive.");

    // Calibrate TSC for profiling before user tasks can preempt the bootstrap task.
    kernel::profiler::calibrate_tsc();

    kernel::runtime::initialize();

    kernel::diagnostic::log::section("Userspace");
    kernel::diagnostic::smoke::run_user_smoke_demo(&mut frame_allocator);
    boot_summary.smoke_tests.mmap = CheckStatus::Pass;
    boot_summary.smoke_tests.file_mmap = CheckStatus::Pass;
    run_post_userspace_diagnostics(&mut boot_summary, &frame_allocator);

    // Main Loop
    loop {
        kernel::runtime::tick();

        // For maximum performance testing, we don't hlt.
        // x86_64::instructions::hlt();
    }
}
