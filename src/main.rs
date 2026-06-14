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

use alloc::vec::Vec;
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

const IOAPIC_MMIO_MAPPING_SIZE: u64 = 4096;

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

    arch::x86_64::enable_interrupts();
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

fn verify_frame_allocator_rules() {
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
    if zero_skip_ok
        && range_tracking_ok
        && duplicate_allocation_ok
        && contiguous_boundaries_ok
        && reserved_exclusion_ok
        && owner_tracking_ok
        && released_frame_reuse_ok
        && owner_coverage_ok
    {
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

fn verify_acpi_parser_rules() {
    assert!(
        kernel::acpi::verify_parser_rules(),
        "ACPI parser self-check must pass"
    );
    crate::log_info!(
        "acpi",
        "ACPI parser self-check passed: rsdp=true root_table=true madt=true"
    );
}

fn verify_acpi_root_table(
    root_pointer: Option<kernel::acpi::RootPointer>,
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    let root_pointer =
        root_pointer.expect("UEFI ACPI RSDP configuration table is required before APIC setup");
    // SAFETY: The RSDP address came from the UEFI configuration table before
    // ExitBootServices, and paging has identity-mapped boot memory ranges.
    let diagnostics: kernel::acpi::Diagnostics =
        unsafe { kernel::acpi::inspect_root_pointer(root_pointer) }
            .expect("UEFI ACPI RSDP and root table must validate before APIC setup");
    let root_table: kernel::acpi::RootTableDiagnostics = diagnostics.root_table();
    let root_table_kind: kernel::acpi::RootTableKind = root_table.kind();
    if let Some(xsdt_address) = diagnostics.xsdt_address() {
        crate::log_info!(
            "acpi",
            "ACPI root table verified: source={} revision={} rsdt={:#x} xsdt={:#x} root_table={} root_address={:#x} root_revision={} root_length={} entries={} checksum=true",
            diagnostics.root_pointer().source().as_str(),
            diagnostics.revision(),
            diagnostics.rsdt_address(),
            xsdt_address,
            root_table_kind.as_str(),
            root_table.physical_address(),
            root_table.revision(),
            root_table.length(),
            root_table.entry_count()
        );
    } else {
        crate::log_info!(
            "acpi",
            "ACPI root table verified: source={} revision={} rsdt={:#x} xsdt=none root_table={} root_address={:#x} root_revision={} root_length={} entries={} checksum=true",
            diagnostics.root_pointer().source().as_str(),
            diagnostics.revision(),
            diagnostics.rsdt_address(),
            root_table_kind.as_str(),
            root_table.physical_address(),
            root_table.revision(),
            root_table.length(),
            root_table.entry_count()
        );
    }
    let madt: kernel::acpi::MadtDiagnostics = diagnostics.madt();
    crate::log_info!(
        "acpi",
        "ACPI MADT verified: address={:#x} revision={} length={} local_apic={:#x} flags={:#x} pc_at_compatible={} entries={} local_apics={} ioapics={} interrupt_source_overrides={} local_apic_nmis={} local_apic_address_overrides={} x2apics={} checksum=true",
        madt.physical_address(),
        madt.revision(),
        madt.length(),
        madt.local_apic_address(),
        madt.flags(),
        madt.pc_at_compatible(),
        madt.entry_count(),
        madt.local_apic_count(),
        madt.ioapic_count(),
        madt.interrupt_source_override_count(),
        madt.local_apic_nmi_count(),
        madt.local_apic_address_override_count(),
        madt.x2apic_count()
    );
    let topology: kernel::acpi::MadtInterruptTopology = madt.topology();
    let ioapic: kernel::acpi::MadtIoApic = topology
        .ioapic(0)
        .expect("ACPI MADT must contain at least one IOAPIC before IOAPIC setup");
    let local_apic: kernel::acpi::MadtLocalApic = topology
        .local_apic(0)
        .expect("ACPI MADT must contain at least one Local APIC before APIC setup");
    let legacy_timer_override: Option<kernel::acpi::MadtInterruptSourceOverride> =
        topology.interrupt_source_override_for_legacy_irq(0);
    let local_apic_nmi: Option<kernel::acpi::MadtLocalApicNmi> = topology.local_apic_nmi(0);
    let x2apic: Option<kernel::acpi::MadtX2Apic> = topology.x2apic(0);
    crate::log_info!(
        "acpi",
        "ACPI interrupt topology verified: retained_local_apics={} retained_ioapics={} retained_interrupt_source_overrides={} retained_local_apic_nmis={} retained_x2apics={} topology_truncated={} local_apic0_processor={} local_apic0_id={} local_apic0_flags={:#x} local_apic0_enabled={} local_apic0_online_capable={} ioapic0_id={} ioapic0_address={:#x} ioapic0_gsi_base={} legacy_irq0_gsi={} legacy_irq0_flags={:#x} legacy_irq1_gsi={} legacy_irq12_gsi={} local_apic_nmi0_lint={} x2apic0_present={} x2apic0_id={} x2apic0_uid={} x2apic0_flags={:#x} x2apic0_enabled={} x2apic0_online_capable={}",
        topology.retained_local_apic_count(),
        topology.retained_ioapic_count(),
        topology.retained_interrupt_source_override_count(),
        topology.retained_local_apic_nmi_count(),
        topology.retained_x2apic_count(),
        topology.is_truncated(),
        local_apic.processor_id(),
        local_apic.apic_id(),
        local_apic.flags(),
        local_apic.is_enabled(),
        local_apic.is_online_capable(),
        ioapic.id(),
        ioapic.physical_address(),
        ioapic.global_system_interrupt_base(),
        topology.global_system_interrupt_for_legacy_irq(0),
        legacy_timer_override.map_or(0, kernel::acpi::MadtInterruptSourceOverride::flags),
        topology.global_system_interrupt_for_legacy_irq(1),
        topology.global_system_interrupt_for_legacy_irq(12),
        local_apic_nmi.map_or(0, kernel::acpi::MadtLocalApicNmi::lint),
        x2apic.is_some(),
        x2apic.map_or(0, kernel::acpi::MadtX2Apic::x2apic_id),
        x2apic.map_or(0, kernel::acpi::MadtX2Apic::processor_uid),
        x2apic.map_or(0, kernel::acpi::MadtX2Apic::flags),
        x2apic.is_some_and(kernel::acpi::MadtX2Apic::is_enabled),
        x2apic.is_some_and(kernel::acpi::MadtX2Apic::is_online_capable)
    );
    configure_apic_routing_provider(&madt, &topology, local_apic, ioapic, frame_allocator);
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

fn verify_kernel_console_pipeline() {
    const PIPELINE_COMMAND: &str = "cat /disk/hello.txt | grep FAT32";

    match kernel::console::verify_pipeline_smoke(PIPELINE_COMMAND) {
        Some(output_lines) if output_lines > 0 => crate::log_info!(
            "console",
            "Pipeline command smoke passed: command=\"{}\" output_lines={}",
            PIPELINE_COMMAND,
            output_lines
        ),
        _ => crate::log_warn!(
            "console",
            "Pipeline command smoke failed: command=\"{}\"",
            PIPELINE_COMMAND
        ),
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

fn read_kernel_file(path: &str) -> Option<Vec<u8>> {
    let metadata = kernel::filesystem::metadata(path).ok()?;
    if metadata.file_type != kernel::filesystem::FileType::Regular {
        return None;
    }

    let descriptor = kernel::filesystem::open(path).ok()?;
    let mut contents = Vec::new();
    contents
        .try_reserve_exact(metadata.size)
        .expect("OOM: failed to reserve kernel file buffer");
    contents.resize(metadata.size, 0);

    let mut bytes_read = 0_usize;
    while bytes_read < metadata.size {
        let read_now = kernel::filesystem::read(descriptor, &mut contents[bytes_read..]).ok()?;
        if read_now == 0 {
            break;
        }
        bytes_read = bytes_read
            .checked_add(read_now)
            .expect("kernel file read byte count overflowed");
    }
    kernel::filesystem::close(descriptor).ok()?;
    contents.truncate(bytes_read);
    Some(contents)
}

fn spawn_user_smoke_task(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
    user_elf_path: &str,
    user_elf_bytes: &[u8],
    user_stack_pages: u64,
) -> u64 {
    let user_address_space =
        kernel::memory::address_space::create_user_address_space(frame_allocator);
    crate::log_info!(
        "memory",
        "User address space prepared: pml4={:#x}",
        user_address_space.level_4_frame().as_u64()
    );
    let user_elf: kernel::elf::LoadedElf = kernel::elf::load_user_program(
        user_address_space,
        frame_allocator,
        user_elf_bytes,
        user_elf_path,
    );
    let user_entry_point = user_elf.entry_point();
    let user_heap_start = user_elf.heap_start();
    let user_stack = kernel::memory::user_stack::allocate_user_stack(
        user_address_space,
        frame_allocator,
        user_stack_pages,
    );
    assert!(
        kernel::memory::user_stack::verify_user_stack_mapping(user_address_space, user_stack),
        "user stack mapping and guard page smoke must pass"
    );
    crate::log_info!(
        "memory",
        "User stack mapping verified: pages={} base={:#x} top={:#x} guard_unmapped=true",
        user_stack.page_count(),
        user_stack.base().as_u64(),
        user_stack.top().as_u64()
    );

    let user_stack_probe = user_stack
        .top()
        .checked_sub(1)
        .expect("user stack top must be above the mapped stack");
    assert!(
        user_address_space.verify_kernel_user_mapping_permissions(
            verify_kernel_filesystem as *const () as usize,
            user_stack_probe.as_usize(),
            user_entry_point.as_usize(),
        ),
        "kernel and user mapping permission smoke must pass"
    );
    crate::log_info!(
        "memory",
        "Kernel/user mapping permission self-check passed: pml4={:#x}",
        user_address_space.level_4_frame().as_u64()
    );
    assert!(
        user_address_space.verify_syscall_user_data_permissions(
            user_stack_probe.as_usize(),
            user_entry_point.as_usize(),
        ),
        "syscall user data permission smoke must pass"
    );
    crate::log_info!("memory", "Syscall user data permission self-check passed.");
    let user_entry_arguments = [user_elf_path, "--storage-smoke"];
    let user_entry_environment = ["MANAOS_BOOT=storage-smoke"];
    let prepared_user_stack = kernel::memory::user_stack::prepare_initial_stack(
        user_address_space,
        user_stack,
        &user_entry_arguments,
        &user_entry_environment,
    );
    crate::log_info!(
        "task",
        "User entry arguments prepared: argc={} argv={:#x} envp={:#x}",
        prepared_user_stack.argument_count(),
        prepared_user_stack.argument_values_pointer().as_u64(),
        prepared_user_stack.environment_values_pointer().as_u64()
    );

    let user_task_id = kernel::task::spawn_user_task(
        frame_allocator,
        user_address_space,
        user_entry_point,
        prepared_user_stack.stack_pointer(),
        user_heap_start,
        kernel::task::UserEntryArguments::new(
            prepared_user_stack.argument_count(),
            prepared_user_stack.argument_values_pointer(),
            prepared_user_stack.environment_values_pointer(),
        ),
    );
    crate::log_info!(
        "task",
        "User task spawned. task_id={} address_space={:#x}",
        user_task_id,
        user_address_space.level_4_frame().as_u64()
    );
    user_task_id
}

fn run_user_smoke_demo(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    kernel::task::set_preemption_enabled(false);

    let user_stack_pages = 4;
    let user_elf_path = "/disk/bin/smoke_demo";
    let user_elf_bytes =
        read_kernel_file(user_elf_path).expect("user smoke ELF must be readable from /disk/bin");
    crate::log_info!(
        "elf",
        "Loading user ELF from filesystem: path={} bytes={}",
        user_elf_path,
        user_elf_bytes.len()
    );
    let user_task_ids = [
        spawn_user_smoke_task(
            frame_allocator,
            user_elf_path,
            &user_elf_bytes,
            user_stack_pages,
        ),
        spawn_user_smoke_task(
            frame_allocator,
            user_elf_path,
            &user_elf_bytes,
            user_stack_pages,
        ),
    ];
    crate::log_info!(
        "task",
        "Multi-user smoke tasks spawned: first={} second={}",
        user_task_ids[0],
        user_task_ids[1]
    );
    for user_task_id in &user_task_ids {
        assert!(
            kernel::task::activate_user_task(*user_task_id),
            "spawned user smoke task must be activatable"
        );
    }
    crate::log_info!(
        "task",
        "Multi-user active set prepared: tasks={}",
        user_task_ids.len()
    );

    let exits = kernel::task::run_active_user_tasks_until_empty(frame_allocator);
    assert_eq!(
        exits.len(),
        user_task_ids.len(),
        "active user lifecycle drain must return every smoke task exit"
    );

    let mut finished = [false; 2];
    for exit in exits {
        crate::log_info!(
            "task",
            "UI resumed after user exit: task={} code={}",
            exit.task_id(),
            exit.exit_code()
        );
        let finished_index = user_task_ids
            .iter()
            .position(|task_id| *task_id == exit.task_id())
            .expect("exited task must belong to the multi-user smoke set");
        assert!(
            !finished[finished_index],
            "user smoke task must not exit twice"
        );
        finished[finished_index] = true;
    }

    assert!(
        finished.iter().all(|is_finished| *is_finished),
        "all user smoke tasks must exit"
    );
    verify_bootstrap_child_exit_collection(user_task_ids);
    crate::log_info!(
        "task",
        "Multi-user preemption smoke passed: tasks={}",
        user_task_ids.len()
    );
    kernel::task::set_preemption_enabled(true);
}

fn verify_bootstrap_child_exit_collection(user_task_ids: [u64; 2]) {
    let parent_task_id = kernel::task::TaskIdentifier::BOOTSTRAP.as_u64();
    let mut collected = [false; 2];
    for _ in 0..user_task_ids.len() {
        let exit = kernel::task::collect_waitable_child_exit(parent_task_id)
            .expect("bootstrap parent must have a waitable user child exit");
        assert_eq!(
            exit.exit_code(),
            0,
            "user smoke child exit status must retain code zero"
        );
        let child_index = user_task_ids
            .iter()
            .position(|task_id| *task_id == exit.task_id())
            .expect("waited child must belong to the user smoke task set");
        assert!(
            !collected[child_index],
            "waited child exit status must be collected once"
        );
        collected[child_index] = true;
    }
    assert!(
        collected.iter().all(|is_collected| *is_collected),
        "bootstrap wait collection must cover every user smoke child"
    );
    assert!(
        kernel::task::collect_waitable_child_exit(parent_task_id).is_none(),
        "bootstrap parent must not collect the same child exit twice"
    );
    crate::log_info!(
        "task",
        "Bootstrap child wait collection verified: parent={} children={}",
        parent_task_id,
        user_task_ids.len()
    );
}

fn verify_scheduler_task_diagnostics(expected_user_tasks: u64) {
    let diagnostics = kernel::task::get_scheduler_diagnostics()
        .expect("scheduler diagnostics must be available after user smoke tasks");
    let states = diagnostics.states();
    verify_scheduler_task_counts(&diagnostics, states, expected_user_tasks);
    verify_scheduler_reclaim_diagnostics(&diagnostics, expected_user_tasks);
    verify_scheduler_user_return_diagnostics(&diagnostics, expected_user_tasks);
    log_scheduler_task_diagnostics(&diagnostics, states);
}

fn verify_scheduler_task_counts(
    diagnostics: &kernel::task::SchedulerDiagnostics,
    states: kernel::task::TaskStateDiagnostics,
    expected_user_tasks: u64,
) {
    assert_eq!(
        diagnostics.user_tasks(),
        expected_user_tasks,
        "scheduler diagnostics must count spawned user tasks"
    );
    assert_eq!(
        diagnostics.active_user_address_spaces(),
        0,
        "finished user tasks must not retain address spaces"
    );
    assert_eq!(
        diagnostics.active_user_tasks(),
        0,
        "finished user tasks must not remain in the active scheduling set"
    );
    assert_eq!(
        diagnostics.retained_user_exit_statuses(),
        expected_user_tasks,
        "finished user tasks must retain waitable exit status records"
    );
    assert_eq!(
        diagnostics.waitable_user_exit_statuses(),
        0,
        "bootstrap parent must collect every waitable user child exit"
    );
    assert_eq!(
        diagnostics.collected_user_exit_statuses(),
        expected_user_tasks,
        "finished user task exit statuses must be marked collected"
    );
    assert_eq!(
        states.finished(),
        expected_user_tasks,
        "all user smoke tasks must be finished"
    );
}

fn verify_scheduler_reclaim_diagnostics(
    diagnostics: &kernel::task::SchedulerDiagnostics,
    expected_user_tasks: u64,
) {
    // The current smoke ELF reclaims five ELF pages, four user stack pages,
    // and the final two-page heap. Private mmap pages are unmapped earlier.
    const SMOKE_RECLAIMED_USER_PAGES_PER_TASK: u64 = 11;
    // The current smoke process touches the program, heap/mmap, and stack
    // windows, leaving ten page-table frames to reclaim with the PML4.
    const SMOKE_RECLAIMED_PAGE_TABLE_PAGES_PER_TASK: u64 = 10;
    // User smoke tasks use the current default guarded kernel stack: four
    // writable pages plus one reserved guard page.
    let expected_reclaimed_user_kernel_stack_writable_pages = expected_user_tasks * 4;
    let expected_reclaimed_user_kernel_stack_virtual_pages = expected_user_tasks * 5;
    let expected_reclaimed_user_pages = expected_user_tasks * SMOKE_RECLAIMED_USER_PAGES_PER_TASK;
    let expected_reclaimed_user_page_table_pages =
        expected_user_tasks * SMOKE_RECLAIMED_PAGE_TABLE_PAGES_PER_TASK;
    assert_eq!(
        diagnostics.reclaimed_user_resource_records(),
        expected_user_tasks,
        "finished user tasks must emit one aggregate resource reclaim record"
    );
    assert_eq!(
        diagnostics.reclaimed_user_address_spaces(),
        expected_user_tasks,
        "finished user tasks must reclaim their address spaces"
    );
    assert_eq!(
        diagnostics.reclaimed_user_pages(),
        expected_reclaimed_user_pages,
        "finished user tasks must return user-owned mapped pages"
    );
    assert_eq!(
        diagnostics.reclaimed_user_page_table_pages(),
        expected_reclaimed_user_page_table_pages,
        "finished user tasks must return user page-table pages"
    );
    assert_eq!(
        diagnostics.reclaimed_user_kernel_stacks(),
        expected_user_tasks,
        "finished user tasks must reclaim their kernel stacks"
    );
    assert_eq!(
        diagnostics.reclaimed_user_kernel_stack_writable_pages(),
        expected_reclaimed_user_kernel_stack_writable_pages,
        "finished user tasks must return writable kernel stack pages"
    );
    assert_eq!(
        diagnostics.reclaimed_user_kernel_stack_virtual_pages(),
        expected_reclaimed_user_kernel_stack_virtual_pages,
        "finished user tasks must return guard-inclusive kernel stack virtual pages"
    );
}

fn verify_scheduler_user_return_diagnostics(
    diagnostics: &kernel::task::SchedulerDiagnostics,
    expected_user_tasks: u64,
) {
    let expected_user_stops = expected_user_tasks * 2;
    assert!(
        diagnostics.timer_preemptions() > 0,
        "user smoke must record timer preemption accounting"
    );
    assert!(
        diagnostics.one_shot_user_entries() >= expected_user_tasks,
        "user smoke must enter user tasks through the lifecycle path"
    );
    assert!(
        diagnostics.timer_user_entries() > 0,
        "user smoke must enter at least one user task from timer scheduling"
    );
    assert_eq!(
        diagnostics.user_entries(),
        diagnostics
            .one_shot_user_entries()
            .saturating_add(diagnostics.timer_user_entries()),
        "aggregate user entries must match lifecycle and timer entry counts"
    );
    assert!(
        diagnostics.user_resumes() > 0,
        "user smoke must record user resume accounting"
    );
    assert_eq!(
        diagnostics.pending_user_exits(),
        0,
        "reported user exits must not remain queued after lifecycle cleanup"
    );
    assert!(
        diagnostics.preemption_enabled(),
        "preemption must be re-enabled after active user lifecycle drain"
    );
    assert_eq!(
        diagnostics.preemption_state(),
        kernel::task::PreemptionStateDiagnostics::Enabled,
        "preemption state must be enabled after active user lifecycle drain"
    );
    assert_eq!(
        diagnostics.user_sleep_blocks(),
        expected_user_tasks,
        "every user smoke task must block once in nanosleep"
    );
    assert_eq!(
        diagnostics.user_sleep_wakes(),
        expected_user_tasks,
        "every sleeping user smoke task must wake once"
    );
    assert_eq!(
        diagnostics.user_return_preemption_window_closes(),
        expected_user_stops,
        "every user smoke sleep and exit must close the preemption return window"
    );
    assert_eq!(
        diagnostics.user_return_stack_sets(),
        expected_user_stops,
        "returnable user stacks must be stored once per user stop"
    );
    assert_eq!(
        diagnostics.user_return_stack_takes(),
        expected_user_stops,
        "returnable user stacks must be consumed once per user stop"
    );
}

fn log_scheduler_task_diagnostics(
    diagnostics: &kernel::task::SchedulerDiagnostics,
    states: kernel::task::TaskStateDiagnostics,
) {
    crate::log_info!(
        "task",
        "Scheduler diagnostics verified: total_tasks={} kernel_tasks={} user_tasks={} ready={} running={} blocked={} finished={} active_user_tasks={} active_user_address_spaces={} pending_user_exits={} retained_user_exit_statuses={} waitable_user_exit_statuses={} collected_user_exit_statuses={} preemption_state={} preemption_enabled={} user_sleep_blocks={} user_sleep_wakes={} user_return_preemption_window_closes={} user_return_stack_sets={} user_return_stack_takes={} reclaimed_user_resource_records={} reclaimed_user_address_spaces={} reclaimed_user_pages={} reclaimed_user_page_table_pages={} reclaimed_user_kernel_stacks={} reclaimed_kernel_stack_writable_pages={} reclaimed_kernel_stack_virtual_pages={} context_switches={} timer_preemptions={} user_entries={} one_shot_user_entries={} timer_user_entries={} user_resumes={} finished_tasks={}",
        diagnostics.total_tasks(),
        diagnostics.kernel_tasks(),
        diagnostics.user_tasks(),
        states.ready(),
        states.running(),
        states.blocked(),
        states.finished(),
        diagnostics.active_user_tasks(),
        diagnostics.active_user_address_spaces(),
        diagnostics.pending_user_exits(),
        diagnostics.retained_user_exit_statuses(),
        diagnostics.waitable_user_exit_statuses(),
        diagnostics.collected_user_exit_statuses(),
        diagnostics.preemption_state().as_str(),
        diagnostics.preemption_enabled(),
        diagnostics.user_sleep_blocks(),
        diagnostics.user_sleep_wakes(),
        diagnostics.user_return_preemption_window_closes(),
        diagnostics.user_return_stack_sets(),
        diagnostics.user_return_stack_takes(),
        diagnostics.reclaimed_user_resource_records(),
        diagnostics.reclaimed_user_address_spaces(),
        diagnostics.reclaimed_user_pages(),
        diagnostics.reclaimed_user_page_table_pages(),
        diagnostics.reclaimed_user_kernel_stacks(),
        diagnostics.reclaimed_user_kernel_stack_writable_pages(),
        diagnostics.reclaimed_user_kernel_stack_virtual_pages(),
        diagnostics.context_switches(),
        diagnostics.timer_preemptions(),
        diagnostics.user_entries(),
        diagnostics.one_shot_user_entries(),
        diagnostics.timer_user_entries(),
        diagnostics.user_resumes(),
        diagnostics.finished_tasks()
    );
}

#[derive(Clone, Copy)]
struct UserTaskSnapshotVerification {
    released_mappings: bool,
    fully_reclaimed: bool,
}

fn verify_scheduler_task_snapshots(expected_user_tasks: u64) {
    let snapshots = kernel::task::get_scheduler_task_snapshots()
        .expect("scheduler task snapshots must be available after user smoke tasks");
    let expected_total_tasks = usize::try_from(expected_user_tasks)
        .expect("expected user task count must fit in usize")
        .checked_add(2)
        .expect("expected total task count must not overflow");
    assert_eq!(
        snapshots.len(),
        expected_total_tasks,
        "scheduler task snapshots must include bootstrap, idle, and smoke user tasks"
    );

    let mut finished_user_tasks = 0_u64;
    let mut fully_reclaimed_user_tasks = 0_u64;
    let mut user_vm_snapshots = 0_u64;
    let mut anonymous_mapping_release_snapshots = 0_u64;
    let mut bootstrap_child_user_tasks = 0_u64;
    let mut collected_user_exit_snapshots = 0_u64;
    for snapshot in snapshots {
        if snapshot.kind() != kernel::task::TaskKindDiagnostics::User {
            continue;
        }
        assert!(
            !snapshot.active(),
            "finished user task snapshots must not be active"
        );
        assert_eq!(
            snapshot.state(),
            kernel::task::TaskState::Finished,
            "user smoke task snapshots must be finished"
        );
        assert_eq!(
            snapshot.parent_task_id(),
            Some(kernel::task::TaskIdentifier::BOOTSTRAP.as_u64()),
            "user smoke task snapshots must retain the bootstrap parent task"
        );
        assert_eq!(
            snapshot.exit_code(),
            Some(0),
            "finished user task snapshots must retain exit code zero"
        );
        assert!(
            snapshot.wait_collected(),
            "finished user task snapshots must show collected wait status"
        );
        collected_user_exit_snapshots = collected_user_exit_snapshots.saturating_add(1);
        bootstrap_child_user_tasks = bootstrap_child_user_tasks.saturating_add(1);
        finished_user_tasks = finished_user_tasks.saturating_add(1);
        let verification = verify_user_task_snapshot(snapshot);
        user_vm_snapshots = user_vm_snapshots.saturating_add(1);
        if verification.released_mappings {
            anonymous_mapping_release_snapshots =
                anonymous_mapping_release_snapshots.saturating_add(1);
        }
        if verification.fully_reclaimed {
            fully_reclaimed_user_tasks = fully_reclaimed_user_tasks.saturating_add(1);
        }
    }
    assert_eq!(
        finished_user_tasks, expected_user_tasks,
        "scheduler snapshots must include every finished user smoke task"
    );
    assert_eq!(
        fully_reclaimed_user_tasks, expected_user_tasks,
        "scheduler snapshots must show user task address spaces and kernel stacks reclaimed"
    );
    assert_eq!(
        user_vm_snapshots, expected_user_tasks,
        "scheduler snapshots must include virtual memory bookkeeping for every user task"
    );
    assert_eq!(
        anonymous_mapping_release_snapshots, expected_user_tasks,
        "scheduler snapshots must show anonymous mmap records released"
    );
    assert_eq!(
        bootstrap_child_user_tasks, expected_user_tasks,
        "scheduler snapshots must show every user task as a bootstrap child"
    );
    assert_eq!(
        collected_user_exit_snapshots, expected_user_tasks,
        "scheduler snapshots must show collected user exit statuses"
    );
    crate::log_info!(
        "task",
        "Scheduler task snapshots verified: rows={} finished_user_tasks={} bootstrap_child_user_tasks={} collected_user_exit_snapshots={} fully_reclaimed_user_tasks={} user_vm_snapshots={} released_mmap_snapshots={}",
        expected_total_tasks,
        finished_user_tasks,
        bootstrap_child_user_tasks,
        collected_user_exit_snapshots,
        fully_reclaimed_user_tasks,
        user_vm_snapshots,
        anonymous_mapping_release_snapshots
    );
}

fn verify_user_task_snapshot(
    snapshot: kernel::task::SchedulerTaskSnapshot,
) -> UserTaskSnapshotVerification {
    const SMOKE_PRIVATE_MAPPING_BYTES: u64 = 16_384;
    const SMOKE_TOTAL_PRIVATE_MAPPING_PAGES: u64 = 6;
    const SMOKE_PEAK_PRIVATE_MAPPING_PAGES: u64 = 3;
    const SMOKE_PEAK_PRIVATE_MAPPING_RECORDS: u64 = 2;

    let user_virtual_memory = snapshot
        .user_virtual_memory()
        .expect("user task snapshots must include virtual memory bookkeeping");
    assert_eq!(
        user_virtual_memory.heap_mapped_pages(),
        2,
        "user smoke task snapshots must retain the final two-page brk state"
    );
    assert_eq!(
        user_virtual_memory.mapping_next_start(),
        kernel::memory::user_layout::USER_MAPPING_BASE + SMOKE_PRIVATE_MAPPING_BYTES,
        "user smoke task snapshots must show one three-page anonymous mmap and one file mmap allocation"
    );
    assert_eq!(
        user_virtual_memory.mapping_total_mapped_pages(),
        SMOKE_TOTAL_PRIVATE_MAPPING_PAGES,
        "user smoke task snapshots must retain total private mmap page allocations"
    );
    assert_eq!(
        user_virtual_memory.mapping_total_released_pages(),
        SMOKE_TOTAL_PRIVATE_MAPPING_PAGES,
        "user smoke task snapshots must retain total private mmap page releases"
    );
    assert_eq!(
        user_virtual_memory.mapping_peak_active_pages(),
        SMOKE_PEAK_PRIVATE_MAPPING_PAGES,
        "user smoke task snapshots must retain mmap active-page high-water marks"
    );
    assert_eq!(
        user_virtual_memory.mapping_peak_active_records(),
        SMOKE_PEAK_PRIVATE_MAPPING_RECORDS,
        "user smoke task snapshots must retain mmap record high-water marks"
    );
    assert_eq!(
        user_virtual_memory.mapping_file_private_map_count(),
        1,
        "user smoke task snapshots must retain file-private mmap call counts"
    );

    UserTaskSnapshotVerification {
        released_mappings: user_virtual_memory.mapping_active_pages() == 0
            && user_virtual_memory.mapping_active_records() == 0,
        fully_reclaimed: !snapshot.address_space_owned() && !snapshot.kernel_stack_owned(),
    }
}

fn record_memory_diagnostics_snapshot(
    frame_allocator: &kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    kernel::memory::diagnostics::record_frame_allocator_snapshot(frame_allocator);
    let diagnostics = kernel::memory::diagnostics::get_frame_allocator_diagnostics()
        .expect("frame allocator diagnostics must be available after recording a snapshot");
    let owners = diagnostics.owners();
    crate::log_info!(
        "memory",
        "Frame allocator diagnostics snapshot: free={} used={} reserved={} page_table={} kernel_heap={} kernel_stack={} user_stack={} user_elf={} user_heap={} user_mapping={} dynamic_kernel_mapping={} ahci_dma={}",
        diagnostics.free(),
        diagnostics.used(),
        diagnostics.reserved(),
        owners.page_table(),
        owners.kernel_heap(),
        owners.kernel_stack(),
        owners.user_stack(),
        owners.user_elf(),
        owners.user_heap(),
        owners.user_mapping(),
        owners.dynamic_kernel_mapping(),
        owners.ahci_dma()
    );
}

fn verify_scheduler_console_command() {
    match kernel::console::verify_command_smoke_contains(
        "tasks",
        &[
            "reclaimed_user_address_spaces=",
            "process_lifecycle:",
            "collected_user_exit_statuses=",
            "one_shot_user_entries=",
            "timer_user_entries=",
            "user_vm_layout:",
            "task_vm:",
            "task_mmap_lifecycle:",
        ],
    ) {
        Some(output_lines) if output_lines >= 15 => crate::log_info!(
            "console",
            "Tasks command smoke passed: command=\"tasks\" output_lines={}",
            output_lines
        ),
        _ => crate::log_warn!("console", "Tasks command smoke failed: command=\"tasks\""),
    }
}

fn verify_memory_console_command() {
    match kernel::console::verify_command_smoke("memory") {
        Some(output_lines) if output_lines >= 3 => crate::log_info!(
            "console",
            "Memory command smoke passed: command=\"memory\" output_lines={}",
            output_lines
        ),
        _ => crate::log_warn!("console", "Memory command smoke failed: command=\"memory\""),
    }
}

fn verify_syscall_trace_console_command() {
    crate::kernel::syscall::set_trace_enabled(false);
    crate::kernel::syscall::reset_trace();
    let reset_ok = kernel::console::verify_command_smoke_contains(
        "syscalls trace reset",
        &["trace: enabled=false", "records=0", "last_number=-"],
    )
    .is_some();
    let enabled_ok = kernel::console::verify_command_smoke_contains(
        "syscalls trace on",
        &["trace: enabled=true", "records=0"],
    )
    .is_some();
    let _traced_result = crate::kernel::syscall::syscall_dispatch(
        crate::kernel::syscall::SYS_GETPID,
        0,
        0,
        0,
        0,
        0,
        0,
    );
    let disabled_ok = kernel::console::verify_command_smoke_contains(
        "syscalls trace off",
        &[
            "trace: enabled=false",
            "records=1",
            "last_number=39",
            "last_result=0x",
        ],
    )
    .is_some();

    if reset_ok && enabled_ok && disabled_ok {
        crate::log_info!(
            "console",
            "Syscall trace controls smoke passed: command=\"syscalls trace\" records=1"
        );
    } else {
        crate::log_warn!(
            "console",
            "Syscall trace controls smoke failed: command=\"syscalls trace\""
        );
    }
}

fn verify_console_status_strip() {
    if kernel::console::verify_status_strip_smoke() {
        crate::log_info!("console", "Console status strip smoke passed.");
    } else {
        crate::log_warn!("console", "Console status strip smoke failed.");
    }
}

#[entry]
fn main() -> Status {
    // ────────────────────────────────────────────────
    // Boot Phase (UEFI Services available)
    // ────────────────────────────────────────────────
    kernel::logger::init();

    log::info!("ManaOS booting (HAL edition)...");

    let framebuffer_info = get_framebuffer_info();

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
    crate::log_info!("serial", "ExitBootServices OK.");
    let mut frame_allocator = kernel::memory::frame_allocator::PhysicalFrameAllocator::new();
    import_boot_memory_map(&mut frame_allocator, mmap.entries());
    verify_frame_allocator_rules();
    verify_kernel_virtual_range_allocator_rules();
    verify_elf_loader_rules();
    verify_acpi_parser_rules();

    // ────────────────────────────────────────────────
    // Kernel Phase (UEFI Services unavailable)
    // ────────────────────────────────────────────────
    crate::log_info!("kernel", "ManaOS Kernel phase started.");

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
    verify_acpi_root_table(acpi_root_pointer, &mut frame_allocator);
    verify_dynamic_kernel_mapping_lifecycle(&mut frame_allocator);
    verify_user_address_space_template(&mut frame_allocator);
    verify_user_address_space_reclaim(&mut frame_allocator);
    kernel::filesystem::initialize();
    crate::log_info!("fs", "Kernel filesystem initialized.");
    verify_kernel_filesystem();
    kernel::driver::storage::init(
        &mut frame_allocator,
        kernel::driver::storage::PciConfigurationAccess::new(
            arch::x86_64::pci_configuration::read_config32,
            arch::x86_64::pci_configuration::write_config32,
        ),
    );
    verify_primary_storage_device();
    if mount_detected_disk_files() {
        verify_mounted_disk_file("/disk/hello.txt");
        verify_kernel_console_pipeline();
    }
    initialize_scheduler(&mut frame_allocator);
    verify_kernel_stack_guard_fault_diagnostics();
    initialize_architecture_and_drivers();

    crate::log_info!("kernel", "ManaOS Kernel is alive.");

    // Calibrate TSC for profiling before user tasks can preempt the bootstrap task.
    kernel::profiler::calibrate_tsc();

    kernel::runtime::initialize();

    run_user_smoke_demo(&mut frame_allocator);
    verify_scheduler_task_diagnostics(2);
    verify_scheduler_task_snapshots(2);
    record_memory_diagnostics_snapshot(&frame_allocator);
    verify_scheduler_console_command();
    verify_memory_console_command();
    verify_syscall_trace_console_command();
    verify_console_status_strip();

    // Main Loop
    loop {
        kernel::runtime::tick();

        // For maximum performance testing, we don't hlt.
        // x86_64::instructions::hlt();
    }
}
