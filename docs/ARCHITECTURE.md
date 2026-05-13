# ManaOS Architecture

## Module Ownership Rules

Each module owns exactly one thing. Before adding code, ask:
"Does this belong to an existing module, or do I need a new one?"

## Data Flow

Interrupt → RingBuffer → Main Loop → State Update → Render

## Dependency Rules (Strictly Enforced)

- `arch/` must NEVER depend on `kernel/`
- `kernel/driver/` may depend on `kernel/memory/`
- `kernel/driver/display/` must NEVER depend on `kernel/driver/input/`
- `main.rs` is the only module that orchestrates the system

## Adding a New Driver (Checklist)

- [ ] Copy `templates/driver.rs.template` to start.
- [ ] Write module responsibility comments in `mod.rs`.
- [ ] Keep all static variables `private`.
- [ ] Interrupt handlers must only `push` to a queue.
- [ ] All processing must occur in `process()` called from the main loop.
