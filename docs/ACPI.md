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
- RSDT/XSDT entry walk for the MADT (`APIC`) table.
- MADT signature, length, and checksum.
- MADT Local APIC address, raw flags, compatibility flags, and
  interrupt-controller entry counts.
- MADT Processor Local APIC, IOAPIC, interrupt source override, Local APIC NMI,
  Local APIC address override, and Processor Local x2APIC diagnostics.

The boot smoke logs the validated root table and MADT diagnostics before any
APIC migration work depends on them.

## Next Steps

1. Define kernel-owned interrupt topology data derived from MADT diagnostics.
2. Wire IOAPIC and Local APIC providers through `main.rs`.
3. Enable IOAPIC routing while keeping `arch/` independent from `kernel/`.
4. Replace legacy PIC routing after IOAPIC validation.
5. Calibrate and move scheduling ticks to the Local APIC timer.
