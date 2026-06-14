# ManaOS ACPI Foundation

ManaOS uses ACPI as the firmware-provided hardware topology source for the
future IOAPIC and Local APIC migration.

## Ownership

- `main.rs` discovers the UEFI ACPI configuration table entry while boot
  services are still available.
- `kernel::acpi` validates the RSDP and the selected RSDT/XSDT root table.
- `arch/` remains responsible for architecture-specific interrupt controller
  programming and must not call back into `kernel::acpi`.

## Current Scope

The current foundation validates:

- RSDP signature.
- RSDP ACPI 1.0 checksum.
- ACPI 2.0 extended RSDP length and checksum when present.
- RSDT/XSDT signature.
- RSDT/XSDT length, checksum, and root entry count.

The boot smoke logs the validated root table before any APIC migration work
depends on it.

## Next Steps

1. Walk the validated RSDT/XSDT entries.
2. Locate and validate the MADT.
3. Parse Local APIC, IOAPIC, interrupt source override, and NMI entries.
4. Expose kernel-side MADT diagnostics without making `arch/` depend on
   `kernel/`.
5. Wire IOAPIC and Local APIC providers through `main.rs`.
