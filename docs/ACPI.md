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
- Architecture-owned APIC routing provider configuration translated by
  `main.rs` from kernel-owned ACPI topology records.
- Dry-run IOAPIC redirection entries for timer, keyboard, and mouse vectors
  derived from the architecture-owned APIC routing provider configuration.
- Guarded IOAPIC MMIO mapping from the composition root before arch-owned
  register access.
- Masked IOAPIC redirection entry staging with IOAPIC version, table range,
  and register readback diagnostics.
- Guarded Local APIC MMIO mapping from the composition root before arch-owned
  EOI-provider diagnostics.
- Local APIC EOI-provider diagnostics for APIC ID, version, LVT capacity, and
  spurious-vector state.
- Unified interrupt EOI dispatch that continues to acknowledge the legacy PIC
  until IOAPIC routing is explicitly activated.

The boot smoke logs the validated root table, MADT diagnostics, retained
interrupt topology, APIC routing provider configuration, and dry-run IOAPIC
redirection plan before staging the planned entries as masked IOAPIC routes.
It also verifies Local APIC and IOAPIC MMIO mapping, Local APIC EOI-provider
diagnostics, and masked IOAPIC redirection readback while keeping hardware
interrupt routing inactive.

## Next Steps

1. Unmask the staged IOAPIC redirection entries and activate APIC EOI dispatch
   under the same validation gate.
2. Add diagnostics proving interrupts are acknowledged through Local APIC EOI
   after IOAPIC routing is active.
3. Replace legacy PIC routing after IOAPIC validation.
4. Calibrate and move scheduling ticks to the Local APIC timer.
