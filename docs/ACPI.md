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
- Bounded MADT topology records for Processor Local APIC, IOAPIC, interrupt
  source override, Local APIC NMI, and Processor Local x2APIC entries.
- Legacy IRQ to global system interrupt resolution derived from MADT interrupt
  source override entries.

The boot smoke logs the validated root table, MADT diagnostics, and retained
interrupt topology before any APIC migration work depends on them.

## Next Steps

1. Wire IOAPIC and Local APIC providers through `main.rs`.
2. Enable IOAPIC routing while keeping `arch/` independent from `kernel/`.
3. Replace legacy PIC routing after IOAPIC validation.
4. Calibrate and move scheduling ticks to the Local APIC timer.
