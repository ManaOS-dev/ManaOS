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
mod boot_interrupts;
mod boot_services;
mod kernel;
mod shared;

use uefi::prelude::*;
use uefi::{
    boot,
    mem::memory_map::{MemoryMap, MemoryType},
};

use crate::boot_services::{
    allocate_backbuffer, find_acpi_root_pointer, framebuffer_format_name, get_framebuffer_info,
    get_framebuffer_size, import_boot_memory_map, load_file,
};
use crate::kernel::diagnostic::summary::{BootSummary, CheckStatus, FramebufferSummary};

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

fn initialize_scheduler(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    kernel::task::initialize();
    let bootstrap_file_descriptors = kernel::filesystem::create_standard_file_descriptor_table();
    kernel::task::replace_current_file_descriptor_table(bootstrap_file_descriptors)
        .expect("scheduler bootstrap task must accept standard file descriptors");
    kernel::task::spawn(frame_allocator, idle_task);
    let task_id = kernel::task::get_current_task_id()
        .expect("scheduler must expose a bootstrap task after initialization");
    crate::log_info!("task", "Scheduler initialized. current_task={}", task_id);
}

fn initialize_architecture_and_drivers() {
    let syscall_entry_address =
        arch::x86_64::SyscallEntryAddress::from_function(kernel::interrupt::syscall_entry);
    arch::init(syscall_entry_address);
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

    boot_interrupts::activate_ioapic_interrupt_routing();
    boot_interrupts::start_local_apic_timer_calibration();
    arch::x86_64::enable_interrupts();
    boot_interrupts::activate_local_apic_timer_ticks();
}

fn run_post_userspace_diagnostics(
    boot_summary: &mut BootSummary,
    frame_allocator: &kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    kernel::diagnostic::log::section("Diagnostics / Smoke Tests");
    let eoi_status = boot_interrupts::verify_apic_eoi_diagnostics();
    boot_summary.ioapic_active = Some(eoi_status.is_ioapic_routing_active());
    boot_summary.apic_eoi_count = Some(eoi_status.apic_count());
    boot_summary.legacy_eoi_count = Some(eoi_status.legacy_count());
    boot_summary.local_apic_enabled = Some(boot_interrupts::verify_interrupt_vector_diagnostics());
    if boot_interrupts::verify_local_apic_timer_post_smoke() {
        boot_summary.timer = Some("Local APIC periodic");
    }

    let expected_user_smoke_tasks = u64::try_from(kernel::diagnostic::smoke::USER_SMOKE_TASK_COUNT)
        .expect("user smoke task count must fit in u64");
    kernel::diagnostic::smoke::verify_scheduler_task_diagnostics(expected_user_smoke_tasks);
    kernel::diagnostic::smoke::verify_scheduler_task_snapshots(expected_user_smoke_tasks);
    kernel::diagnostic::boot_smoke::record_scheduler_boot_summary(boot_summary);
    kernel::diagnostic::smoke::record_memory_diagnostics_snapshot(frame_allocator);
    boot_summary.free_frames = Some(frame_allocator.statistics().free);
    kernel::diagnostic::boot_smoke::record_console_smoke_summary(boot_summary);
    boot_summary.emit();
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
    boot_summary.frame_allocator =
        CheckStatus::from_bool(kernel::diagnostic::boot_smoke::verify_frame_allocator_rules());
    kernel::diagnostic::boot_smoke::verify_memory_address_wrapper_rules();
    kernel::diagnostic::boot_smoke::verify_kernel_virtual_range_allocator_rules();
    kernel::diagnostic::boot_smoke::verify_elf_loader_rules();

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
    kernel::diagnostic::boot_smoke::verify_dynamic_kernel_mapping_lifecycle(&mut frame_allocator);
    kernel::diagnostic::boot_smoke::verify_user_address_space_template(&mut frame_allocator);
    kernel::diagnostic::boot_smoke::verify_user_address_space_reclaim(&mut frame_allocator);

    kernel::diagnostic::log::section("ACPI");
    let acpi_parser_passed = kernel::diagnostic::acpi::verify_parser_rules();
    let acpi_root_passed =
        boot_interrupts::verify_acpi_root_table(acpi_root_pointer, &mut frame_allocator);
    boot_summary.acpi = CheckStatus::from_bool(acpi_parser_passed && acpi_root_passed);

    kernel::diagnostic::log::section("Filesystem");
    kernel::filesystem::initialize();
    crate::log_info!("fs", "Kernel filesystem initialized.");
    kernel::diagnostic::boot_smoke::verify_kernel_filesystem();

    kernel::diagnostic::log::section("Storage");
    kernel::driver::storage::init(
        &mut frame_allocator,
        kernel::driver::storage::PciConfigurationAccess::new(
            arch::x86_64::pci_configuration::read_config32,
            arch::x86_64::pci_configuration::write_config32,
        ),
    );
    kernel::diagnostic::boot_smoke::verify_primary_storage_device();
    kernel::diagnostic::boot_smoke::record_storage_boot_summary(&mut boot_summary);

    kernel::diagnostic::log::section("Scheduler");
    initialize_scheduler(&mut frame_allocator);
    kernel::diagnostic::boot_smoke::verify_kernel_stack_guard_fault_diagnostics();

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
