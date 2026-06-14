# ManaOS Task Priority

This document sorts the current unfinished work by implementation difficulty and
dependency depth. It is intentionally focused on the next engineering order,
not on product value.

## Hardest First Order

1. ACPI and APIC interrupt migration
   - Parse ACPI RSDP and RSDT/XSDT.
   - Parse MADT entries.
   - Enable IOAPIC routing.
   - Replace legacy PIC routing.
   - Calibrate and switch scheduling ticks to the Local APIC timer.
   - Reason: this changes early boot discovery, interrupt topology, timer
     ownership, and architecture/kernel wiring at the same time.

2. Full user process lifecycle
   - Add `execve`.
   - Add user-visible `wait` or `waitpid`.
   - Add a minimal user shell process.
   - Extend preemptive scheduling across general process lifecycle paths.
   - Reason: this crosses ELF loading, syscall ABI, address-space ownership,
     file descriptors, parent-child metadata, and scheduler cleanup.

3. Remaining per-task kernel stack completion
   - Represent bootstrap and architecture-owned TSS/IST stacks in diagnostics.
   - Finish guard-page ownership for non-scheduler stacks.
   - Reason: this touches fault handling and low-level stack safety, but the
     scheduler-owned stack path is already in place.

4. Typed physical and virtual address wrapper sweep
   - Replace remaining raw `u64` address leakage across subsystem boundaries.
   - Reason: this has broad call-site churn but can be staged behind existing
     newtypes and tests.

5. Synchronization and interrupt-time lock audit
   - Define interrupt-callable APIs.
   - Add lock ordering notes.
   - Split queue producer/consumer assumptions where needed.
   - Reason: this is broad but mostly diagnostic and structural until new SMP
     or APIC paths make the risks observable.

6. Input/display/userland quality work
   - Keyboard layout boundary, key releases, modifier state, text console,
     damage tracking, formatting helpers, and CI build checks.
   - Reason: these are important but less blocking for the kernel execution
     model.

## Current Selection

The active task is ACPI and APIC interrupt migration. ACPI root discovery,
RSDT/XSDT validation, MADT validation, bounded MADT topology diagnostics, and
architecture-owned APIC routing provider configuration are now proven by storage
smoke. The next slice should program IOAPIC redirection entries from that
provider configuration while preserving the `arch/` to `kernel/` dependency
boundary.
