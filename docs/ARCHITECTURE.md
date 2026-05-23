# ManaOS Architecture

## Module Ownership Rules

Each module owns exactly one thing. Before adding code, ask:
"Does this belong to an existing module, or do I need a new one?"

`main.rs` is allowed to know both architecture-specific and kernel modules because
it is the composition root. Other modules must keep their ownership boundary
small and explicit.

## Data Flow

Hardware interrupt -> `arch` interrupt handler -> registered callback ->
`kernel::interrupt` bridge -> private kernel queue or scheduler -> main loop
processor -> state update -> render command -> display driver.

The important rule is that `arch/` does not know which kernel subsystem receives
an event. It only reads hardware state, acknowledges the interrupt controller,
and calls a callback registered by `main.rs`.

## Dependency Rules (Strictly Enforced)

- `arch/` must NEVER depend on `kernel/`
- `kernel/driver/` may depend on `kernel/memory/`
- `kernel/driver/display/` must NEVER depend on `kernel/driver/input/`
- `main.rs` is the only module that orchestrates the system

## Interrupt Wiring

Interrupt handlers in `arch/x86_64/interrupt_descriptor_table.rs` must stay tiny:

- read the required hardware byte or tick state
- acknowledge the interrupt controller
- call a registered callback when present

The callback registration is currently wired in `main.rs`:

- timer tick -> `kernel::interrupt::process_timer_tick`
- keyboard byte -> `kernel::interrupt::push_keyboard_byte`
- mouse byte -> `kernel::interrupt::push_mouse_byte`

This preserves the dependency direction:

```text
main.rs -> arch/
main.rs -> kernel/
arch/   -> registered callbacks only
kernel/ -> no dependency on arch internals except explicit architecture APIs
```

The architecture side exposes one `InterruptProcessors` struct and one
`register_processors(...)` function. `main.rs` builds that struct because it is
the only composition root. `kernel::interrupt` provides thin bridge functions so
`main.rs` does not wire directly into task or input internals.

## Current Known Design Debt

- Local APIC timer and IOAPIC routing are represented as architecture backends,
  but the boot path still uses the legacy programmable interval timer and 8259
  interrupt controllers until ACPI MADT parsing is added.
- Ring 3 selectors, the initial `iretq` transition path, a fixed user stack
  mapping, and minimal `SYSCALL`/`SYSRET` MSR setup are present. Real syscall
  dispatch, ELF loading, and per-process address spaces are still Phase 6 work.
- Cursor rendering is display-owned, but the cursor shape is still a simple
  placeholder rectangle.

## Adding a New Driver (Checklist)

- [ ] Copy `templates/driver.rs.template` to start.
- [ ] Write module responsibility comments in `mod.rs`.
- [ ] Keep all static variables `private`.
- [ ] Interrupt handlers must only read hardware, acknowledge, and dispatch to a registered callback.
- [ ] All processing must occur in `process()` called from the main loop.
