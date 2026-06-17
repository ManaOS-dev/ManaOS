//! ACPI boot diagnostics and structured log output.

use crate::kernel;
use crate::kernel::diagnostic::log::{LogField, LogLevel};

/// ACPI tables verified during boot and required for APIC setup.
#[derive(Clone, Copy)]
pub struct VerifiedAcpiTables {
    madt: kernel::acpi::MadtDiagnostics,
    topology: kernel::acpi::MadtInterruptTopology,
    local_apic: kernel::acpi::MadtLocalApic,
    ioapic: kernel::acpi::MadtIoApic,
}

impl VerifiedAcpiTables {
    /// Return the verified MADT diagnostics.
    pub const fn madt(self) -> kernel::acpi::MadtDiagnostics {
        self.madt
    }

    /// Return the retained MADT interrupt topology.
    pub const fn topology(self) -> kernel::acpi::MadtInterruptTopology {
        self.topology
    }

    /// Return the Local APIC entry selected for boot routing.
    pub const fn local_apic(self) -> kernel::acpi::MadtLocalApic {
        self.local_apic
    }

    /// Return the IOAPIC entry selected for boot routing.
    pub const fn ioapic(self) -> kernel::acpi::MadtIoApic {
        self.ioapic
    }
}

/// Verify ACPI parser self-check fixtures.
pub fn verify_parser_rules() -> bool {
    assert!(
        kernel::acpi::verify_parser_rules(),
        "ACPI parser self-check must pass"
    );
    crate::log_info!(
        "acpi",
        "ACPI parser self-check passed: rsdp=true root_table=true madt=true physical_addresses_typed=true"
    );
    true
}

/// Inspect and log the ACPI root table and MADT topology required for APIC setup.
pub fn inspect_verified_root_table(
    root_pointer: Option<kernel::acpi::RootPointer>,
) -> VerifiedAcpiTables {
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
    VerifiedAcpiTables {
        madt,
        topology,
        local_apic,
        ioapic,
    }
}

fn log_acpi_root_table(
    diagnostics: &kernel::acpi::Diagnostics,
    root_table: kernel::acpi::RootTableDiagnostics,
) {
    let source = diagnostics.root_pointer().source().as_str();
    let revision = diagnostics.revision();
    let rsdt_address = diagnostics.rsdt_address().as_u64();
    let root_table_kind: kernel::acpi::RootTableKind = root_table.kind();
    let root_table_label = root_table_kind.as_str();
    let root_address = root_table.physical_address().as_u64();
    let root_revision = root_table.revision();
    let root_length = root_table.length();
    let root_entry_count = root_table.entry_count();
    if let Some(xsdt_address) = diagnostics.xsdt_address() {
        let xsdt_address = xsdt_address.as_u64();
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
                LogField::new("root_physical_addresses_typed", format_args!("true")),
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
                LogField::new("root_physical_addresses_typed", format_args!("true")),
            ],
        );
    }
}

fn log_acpi_madt(madt: &kernel::acpi::MadtDiagnostics) {
    let madt_address = madt.physical_address().as_u64();
    let madt_revision = madt.revision();
    let madt_length = madt.length();
    let local_apic_address = madt.local_apic_address().as_u64();
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
            LogField::new("madt_physical_addresses_typed", format_args!("true")),
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
    let ioapic0_address = ioapic.physical_address().as_u64();
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
            LogField::new("topology_physical_addresses_typed", format_args!("true")),
        ],
    );
}
