//! Boot-time interrupt routing and timer diagnostics.

use crate::kernel::diagnostic::log::{LogField, LogLevel};
use crate::{arch, kernel};

const LOCAL_APIC_MMIO_MAPPING_SIZE: u64 = 4096;
const IOAPIC_MMIO_MAPPING_SIZE: u64 = 4096;
/// Verify ACPI tables and configure APIC interrupt routing.
pub(crate) fn verify_acpi_root_table(
    root_pointer: Option<kernel::acpi::RootPointer>,
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) -> bool {
    let tables = kernel::diagnostic::acpi::inspect_verified_root_table(root_pointer);
    configure_apic_routing_provider(
        &tables.madt(),
        &tables.topology(),
        tables.local_apic(),
        tables.ioapic(),
        frame_allocator,
    );
    true
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
    kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "arch",
        format_args!("APIC routing provider configured"),
        &[
            LogField::new("configured", format_args!("{}", status.is_configured())),
            LogField::new(
                "routing_active",
                format_args!("{}", status.is_routing_active()),
            ),
            LogField::new(
                "local_apic_supported",
                format_args!("{}", status.has_local_apic_support()),
            ),
            LogField::new(
                "local_apic_address",
                format_args!("{:#x}", configured_local_apic.physical_address()),
            ),
            LogField::new(
                "local_apic_id",
                format_args!("{}", configured_local_apic.apic_id()),
            ),
            LogField::new(
                "local_apic_enabled",
                format_args!("{}", configured_local_apic.is_enabled()),
            ),
            LogField::new(
                "local_apic_online_capable",
                format_args!("{}", configured_local_apic.is_online_capable()),
            ),
            LogField::new("ioapic_id", format_args!("{}", configured_ioapic.id())),
            LogField::new(
                "ioapic_address",
                format_args!("{:#x}", configured_ioapic.physical_address()),
            ),
            LogField::new(
                "ioapic_gsi_base",
                format_args!("{}", configured_ioapic.global_system_interrupt_base()),
            ),
            LogField::new(
                "legacy_irq_routes",
                format_args!("{}", status.legacy_irq_route_count()),
            ),
            LogField::new(
                "legacy_irq0_gsi",
                format_args!("{}", status.legacy_irq0_global_system_interrupt()),
            ),
            LogField::new(
                "legacy_irq0_flags",
                format_args!("{:#x}", status.legacy_irq0_flags()),
            ),
            LogField::new(
                "legacy_irq1_gsi",
                format_args!("{}", status.legacy_irq1_global_system_interrupt()),
            ),
            LogField::new(
                "legacy_irq12_gsi",
                format_args!("{}", status.legacy_irq12_global_system_interrupt()),
            ),
            LogField::new("route_truncated", format_args!("{}", status.is_truncated())),
        ],
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
    log_ioapic_timer_redirection_plan(
        status,
        redirection_plan,
        first_redirection_entry,
        timer_redirection_entry,
    );
    log_ioapic_input_redirection_plan(keyboard_redirection_entry, mouse_redirection_entry);
}

fn log_ioapic_timer_redirection_plan(
    status: arch::x86_64::interrupt_controller::ApicRoutingProviderStatus,
    redirection_plan: arch::x86_64::interrupt_controller::IoApicRedirectionPlan,
    first_redirection_entry: arch::x86_64::interrupt_controller::IoApicRedirectionEntry,
    timer_redirection_entry: arch::x86_64::interrupt_controller::IoApicRedirectionEntry,
) {
    kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "arch",
        format_args!("IOAPIC redirection plan verified"),
        &[
            LogField::new(
                "entries",
                format_args!("{}", redirection_plan.entry_count()),
            ),
            LogField::new(
                "truncated",
                format_args!("{}", redirection_plan.is_truncated()),
            ),
            LogField::new(
                "routing_active",
                format_args!("{}", status.is_routing_active()),
            ),
            LogField::new(
                "first_irq",
                format_args!("{}", first_redirection_entry.legacy_irq()),
            ),
            LogField::new(
                "timer_irq",
                format_args!("{}", timer_redirection_entry.legacy_irq()),
            ),
            LogField::new(
                "timer_gsi",
                format_args!("{}", timer_redirection_entry.global_system_interrupt()),
            ),
            LogField::new(
                "timer_vector",
                format_args!("{}", timer_redirection_entry.vector()),
            ),
            LogField::new(
                "timer_table_index",
                format_args!("{}", timer_redirection_entry.table_index()),
            ),
            LogField::new(
                "timer_low_register",
                format_args!("{:#x}", timer_redirection_entry.low_register()),
            ),
            LogField::new(
                "timer_high_register",
                format_args!("{:#x}", timer_redirection_entry.high_register()),
            ),
            LogField::new(
                "timer_low_value",
                format_args!("{:#x}", timer_redirection_entry.low_value()),
            ),
            LogField::new(
                "timer_high_value",
                format_args!("{:#x}", timer_redirection_entry.high_value()),
            ),
            LogField::new(
                "timer_active_low",
                format_args!("{}", timer_redirection_entry.is_active_low()),
            ),
            LogField::new(
                "timer_level_triggered",
                format_args!("{}", timer_redirection_entry.is_level_triggered()),
            ),
            LogField::new(
                "timer_masked",
                format_args!("{}", timer_redirection_entry.is_masked()),
            ),
        ],
    );
}

fn log_ioapic_input_redirection_plan(
    keyboard_redirection_entry: arch::x86_64::interrupt_controller::IoApicRedirectionEntry,
    mouse_redirection_entry: arch::x86_64::interrupt_controller::IoApicRedirectionEntry,
) {
    kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "arch",
        format_args!("IOAPIC input redirection plan verified"),
        &[
            LogField::new(
                "keyboard_irq",
                format_args!("{}", keyboard_redirection_entry.legacy_irq()),
            ),
            LogField::new(
                "keyboard_gsi",
                format_args!("{}", keyboard_redirection_entry.global_system_interrupt()),
            ),
            LogField::new(
                "keyboard_vector",
                format_args!("{}", keyboard_redirection_entry.vector()),
            ),
            LogField::new(
                "keyboard_table_index",
                format_args!("{}", keyboard_redirection_entry.table_index()),
            ),
            LogField::new(
                "keyboard_low_register",
                format_args!("{:#x}", keyboard_redirection_entry.low_register()),
            ),
            LogField::new(
                "mouse_irq",
                format_args!("{}", mouse_redirection_entry.legacy_irq()),
            ),
            LogField::new(
                "mouse_gsi",
                format_args!("{}", mouse_redirection_entry.global_system_interrupt()),
            ),
            LogField::new(
                "mouse_vector",
                format_args!("{}", mouse_redirection_entry.vector()),
            ),
            LogField::new(
                "mouse_table_index",
                format_args!("{}", mouse_redirection_entry.table_index()),
            ),
            LogField::new(
                "mouse_low_register",
                format_args!("{:#x}", mouse_redirection_entry.low_register()),
            ),
        ],
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
    kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "arch",
        format_args!("IOAPIC redirection staging verified"),
        &[
            LogField::new(
                "entries",
                format_args!("{}", staging_status.planned_entry_count()),
            ),
            LogField::new(
                "staged",
                format_args!("{}", staging_status.staged_entry_count()),
            ),
            LogField::new(
                "readback_matches",
                format_args!("{}", staging_status.readback_matches()),
            ),
            LogField::new(
                "routing_active",
                format_args!(
                    "{}",
                    arch::x86_64::interrupt_controller::has_ioapic_routing()
                ),
            ),
            LogField::new(
                "masked",
                format_args!("{}", staging_status.all_entries_masked()),
            ),
            LogField::new(
                "ioapic_version",
                format_args!("{:#x}", staging_status.version()),
            ),
            LogField::new(
                "max_redirection_entry",
                format_args!("{}", staging_status.maximum_redirection_entry()),
            ),
            LogField::new(
                "out_of_range_entries",
                format_args!("{}", staging_status.out_of_range_entry_count()),
            ),
            LogField::new(
                "timer_low_readback",
                format_args!("{:#x}", staging_status.timer_low_readback()),
            ),
            LogField::new(
                "timer_high_readback",
                format_args!("{:#x}", staging_status.timer_high_readback()),
            ),
            LogField::new(
                "keyboard_low_readback",
                format_args!("{:#x}", staging_status.keyboard_low_readback()),
            ),
            LogField::new(
                "keyboard_high_readback",
                format_args!("{:#x}", staging_status.keyboard_high_readback()),
            ),
            LogField::new(
                "mouse_low_readback",
                format_args!("{:#x}", staging_status.mouse_low_readback()),
            ),
            LogField::new(
                "mouse_high_readback",
                format_args!("{:#x}", staging_status.mouse_high_readback()),
            ),
        ],
    );
}

/// Activate IOAPIC interrupt routing after masked staging.
pub(crate) fn activate_ioapic_interrupt_routing() {
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

/// Verify APIC and legacy EOI diagnostics after boot smoke interrupts.
pub(crate) fn verify_apic_eoi_diagnostics(
) -> arch::x86_64::interrupt_controller::EndOfInterruptStatus {
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

/// Verify spurious and unexpected interrupt vector diagnostics.
pub(crate) fn verify_interrupt_vector_diagnostics() -> bool {
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

mod timer;

pub(crate) use timer::{
    activate_local_apic_timer_ticks, start_local_apic_timer_calibration,
    verify_local_apic_timer_post_smoke,
};
