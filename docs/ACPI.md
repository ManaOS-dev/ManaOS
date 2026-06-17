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
- ACPI RSDP, root-table, MADT, Local APIC, and IOAPIC physical addresses stay
  typed as `PhysAddr` inside parser diagnostics until boot logging or APIC
  routing setup crosses a final formatting or architecture-MMIO boundary.
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
- IOAPIC route activation for timer, keyboard, and mouse interrupts with
  unmasked redirection readback diagnostics.
- Legacy PIC fallback boundary diagnostics proving normal APIC boots leave the
  legacy PIC masked and fallback-disabled before CPU interrupts are enabled.
- Local APIC EOI counter diagnostics proving timer interrupts are acknowledged
  through APIC EOI after IOAPIC routing activation.
- Masked Local APIC timer calibration diagnostics that arm the timer without
  unmasking its interrupt and compare the count delta against PIT ticks.
- Periodic Local APIC timer activation that masks the IOAPIC PIT timer route and
  keeps scheduler ticks on the existing vector 32 interrupt path.
- Local APIC timer calibration and active status diagnostics retain the timer
  MMIO base as `ApicMmioAddress` until the final serial-log formatting
  boundary.
- Local APIC spurious vector 255 setup plus IDT counters for spurious and
  unexpected external vectors.

The boot smoke logs the validated root table, MADT diagnostics, retained
interrupt topology, APIC routing provider configuration, and dry-run IOAPIC
redirection plan before staging the planned entries as masked IOAPIC routes.
It also verifies that ACPI physical addresses stay typed before diagnostic
formatting, Local APIC and IOAPIC MMIO mapping, Local APIC EOI-provider
diagnostics, masked IOAPIC redirection readback, IOAPIC route activation, and
post-activation APIC EOI counts. Normal APIC boots also assert that the legacy
PIC backend remains masked with fallback delivery disabled before interrupts
are enabled. The smoke path now also proves that a masked Local APIC timer
sample decrements, the IOAPIC PIT timer route is masked, and periodic Local
APIC timer ticks continue to drive scheduler progress while the timer MMIO base
remains typed before diagnostic output. It also asserts that the Local APIC
spurious vector matches the IDT diagnostic vector and that boot does not
observe spurious or unexpected external interrupts.

## Next Steps

1. Continue full process lifecycle work now that timer-driven user preemption no
   longer depends on the PIT route.
