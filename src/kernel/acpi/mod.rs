//! # `kernel::acpi`
//!
//! ## Owns
//! - ACPI root pointer validation
//! - RSDT/XSDT root table diagnostics
//! - MADT interrupt-controller topology diagnostics
//!
//! ## Does NOT own
//! - UEFI configuration table discovery (-> `main.rs`)
//! - Architecture-specific interrupt controller programming (-> `arch`)
//! - Kernel interrupt routing decisions (-> `kernel::interrupt`)
//!
//! ## Public API
//! - [`RootPointer`] - UEFI-provided ACPI RSDP location
//! - [`RootPointerSource`] - Source configuration table for the RSDP
//! - [`Diagnostics`] - Validated ACPI root-table diagnostics
//! - [`MadtDiagnostics`] - Validated MADT interrupt-controller diagnostics
//! - [`MadtInterruptTopology`] - Retained MADT topology records for APIC setup
//! - [`MadtLocalApic`] - Processor Local APIC topology record
//! - [`MadtIoApic`] - IOAPIC topology record
//! - [`MadtInterruptSourceOverride`] - Legacy IRQ override topology record
//! - [`MadtLocalApicNmi`] - Local APIC NMI topology record
//! - [`MadtX2Apic`] - Processor Local x2APIC topology record
//! - [`inspect_root_pointer`] - Validate the RSDP and root table
//! - [`verify_parser_rules`] - Boot-time parser self-check

mod parser;

pub use parser::{
    inspect_root_pointer, verify_parser_rules, Diagnostics, MadtDiagnostics,
    MadtInterruptSourceOverride, MadtInterruptTopology, MadtIoApic, MadtLocalApic,
    MadtLocalApicNmi, MadtX2Apic, RootPointer, RootPointerSource, RootTableDiagnostics,
    RootTableKind,
};
