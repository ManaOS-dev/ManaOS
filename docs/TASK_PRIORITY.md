# ManaOS Task Priority

This document sorts the current unfinished work by implementation difficulty and
dependency depth. It is intentionally focused on the next engineering order,
not on product value.

## Remaining High-Risk Order

1. Full user process lifecycle
   - Promote the smoke-started user shell into an interactive process.
   - Extend preemptive scheduling across general process lifecycle paths.
   - Reason: this crosses ELF loading, syscall ABI, address-space ownership,
     file descriptors, parent-child metadata, and scheduler cleanup.

2. Remaining per-task kernel stack completion
   - Represent bootstrap and architecture-owned TSS/IST stacks in diagnostics.
   - Finish guard-page ownership for non-scheduler stacks.
   - Reason: this touches fault handling and low-level stack safety, but the
     scheduler-owned stack path is already in place.

3. Typed physical and virtual address wrapper sweep
   - Replace remaining raw `u64` address leakage across subsystem boundaries.
   - Reason: this has broad call-site churn but can be staged behind existing
     newtypes and tests.

4. Synchronization and interrupt-time lock audit
   - Define interrupt-callable APIs.
   - Add lock ordering notes.
   - Split queue producer/consumer assumptions where needed.
   - Reason: this is broad but mostly diagnostic and structural until new SMP
     or APIC paths make the risks observable.

5. Storage mutation and parser test expansion
   - Add GPT and FAT32 parser fixtures.
   - Add FAT32 mutation semantics for create, grow, truncate, unlink, and
     directory operations.
   - Add storage reliability counters, retry policy, and write smoke coverage.
   - Reason: the read path is stable enough to protect with tests before the
     write path starts mutating disk images.

6. Input/display/userland quality work
   - Keyboard layout boundary, key releases, modifier state, text console,
     damage tracking, formatting helpers, and CI build checks.
   - Reason: these are important but less blocking for the kernel execution
     model.

## Current Selection

ACPI and APIC interrupt migration is no longer the active selection. ACPI root
discovery, RSDT/XSDT validation, MADT validation, APIC routing provider
configuration, IOAPIC route activation, legacy PIC fallback masking, Local APIC
timer calibration, periodic Local APIC scheduler ticks, and spurious/unexpected
external vector diagnostics are proven by storage smoke.

The active selection is now full user process lifecycle work. The kernel-side
`execve` contract, cleanup invariants, successful self-replacement path,
current-directory preservation, argv/envp-capable `spawn`, nonblocking
`waitpid(WNOHANG)`, and blocking `waitpid(WAIT_ANY)` child collection smoke,
including nonzero child status encoding, initial-process reparenting for
orphaned children, safe finished-task resource reclamation after exit record
retention, process-owned descriptor table inheritance, close-on-exec child
filtering, `execve` replacement-state diagnostics in `tasks` output, and the
post-smoke experimental `user_shell` launch with fixed-buffer stdin EOF handling
and heap-free whitespace tokenization, fixed-buffer argv construction, and
absolute and relative path execution smoke, `pwd` execution through the
userland runtime path API, plus bounded command error smoke for empty input,
token overflow, argument-buffer exhaustion, and missing commands, are documented in
[`PROCESS_LIFECYCLE.md`](PROCESS_LIFECYCLE.md). Continue with small runtime
slices:

1. keep the smoke-started userland shell alive once stdin is keyboard-backed;
2. extend timer preemption across general spawned user process lifecycles;
3. update scheduler diagnostics whenever lifecycle state gains a new transition.

Prefer docs, diagnostics, and narrow smoke assertions before broad syscall
surface expansion.
