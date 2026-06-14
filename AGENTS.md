# ManaOS — Agent Coding Rules

This file defines **mandatory rules** for all AI assistants contributing to ManaOS.
Do not deviate from these rules unless explicitly instructed by the project owner.

---

## 1. Directory Structure

Use `Get-ChildItem -Path src\ -File -Recurse | Resolve-Path -Relative` (PowerShell) or `find src/ -type f` (Linux/macOS) to inspect the directory structure.
---

## 2. Naming Rules

### Naming Clarity

Avoid unclear local abbreviations in identifiers. Domain-standard acronyms are
allowed when they are clearer than spelling out the term.

| Avoid                             | Required / Allowed                                      |
| --------------------------------- | ------------------------------------------------------- |
| `mem/`                            | `memory/`                                               |
| `fb_info`                         | `framebuffer_info`                                      |
| `let (h, v) = ...`                | `let (width, height) = ...`                             |
| `handle_mouse()`                  | `process_packets()`                                     |
| `handle_keyboard()`               | `process_input()`                                       |
| `mod_impl`                        | A descriptive name (e.g., `state`, `packet`, `decoder`) |

Allowed domain acronyms include `PCI`, `AHCI`, `GPT`, `FAT32`, `UEFI`, `GDT`,
`IDT`, `GOP`, `PIC`, `PIT`, `APIC`, `IOAPIC`, `LBA`, `FIS`, `DMA`, and `PRDT`.
Prefer these acronyms in log categories and concise diagnostic messages.

### Function Naming

- Interrupt handlers: `push_*` only (e.g., `push_byte`, `push_scancode`)
- Main loop processors: `process_*` (e.g., `process_packets`, `process_input`)
- Readers: `get_*` (e.g., `get_state`, `get_position`)
- Initializers: `init` or `initialize`

---

## 3. Module Rules

### Each module owns exactly ONE responsibility

Every `mod.rs` MUST start with a module-level doc comment declaring ownership:

```rust
//! # kernel::driver::input::mouse
//!
//! ## Owns
//! - PS/2 mouse byte queue (interrupt → main loop)
//! - Mouse position and button state
//!
//! ## Does NOT own
//! - Cursor rendering (→ kernel::driver::display::cursor)
//! - Hardware initialization details (→ packet.rs)
//!
//! ## Public API
//! - [`init`] — Initialize PS/2 mouse hardware
//! - [`push_byte`] — Called from interrupt handler only
//! - [`process_packets`] — Called from main loop only
//! - [`get_state`] — Read current mouse state
```

### `mod.rs` must be thin

```rust
// ✅ Correct: mod.rs only declares modules and re-exports
mod packet;
mod state;

pub use state::MouseState;

pub fn init() { ... }
pub fn push_byte(byte: u8) { ... }
pub fn process_packets() { ... }
pub fn get_state() -> MouseState { ... }
```

```rust
// ❌ Wrong: business logic inside mod.rs
pub mod packet;        // exposes internals
pub mod state;         // exposes internals
pub use packet::*;     // glob re-export hides public API
```

### Dependency Rules (never break these)

```
arch/  →  must NOT depend on kernel/
kernel/driver/display/  →  must NOT depend on kernel/driver/input/
kernel/driver/  →  may depend on kernel/memory/ and kernel/sync/
main.rs  →  the only file that wires everything together
```

### Interrupt Wiring

`arch/` interrupt handlers must never call `kernel::...` directly.
They may only read hardware state, acknowledge the interrupt controller, and call
a function pointer or callback registered by `main.rs`.

```rust
// ✅ Correct: arch handler dispatches through a registered callback
extern "x86-interrupt" fn mouse_interrupt_handler(_: InterruptStackFrame) {
    let byte = read_mouse_byte();
    call_mouse_byte_processor(byte);
    // SAFETY: interrupt controller EOI must be sent after every hardware interrupt.
    unsafe { notify_end_of_interrupt(InterruptIndex::Mouse) };
}
```

`main.rs` owns the wiring from architecture callbacks to kernel processors:

```rust
interrupt_descriptor_table::register_processors(
    interrupt_descriptor_table::InterruptProcessors {
        timer_tick: kernel::interrupt::process_timer_tick,
        keyboard_byte: kernel::interrupt::push_keyboard_byte,
        mouse_byte: kernel::interrupt::push_mouse_byte,
    },
);
```

`kernel::interrupt` owns kernel-side interrupt event routing. It may call
`kernel::task` and `kernel::driver::input`, but it must not depend on `arch/`.

Task switching follows the same composition-root rule. `arch/` owns assembly
entry points and segment selectors, `main.rs` registers them with
`kernel::task`, and scheduler code calls only the registered task architecture
provider.

---

## 4. Static Variables

All statics MUST be private. Expose state only through functions.

```rust
// ✅ Correct
static STATE: Mutex<MouseState> = Mutex::new(MouseState::new());
pub fn get_state() -> MouseState { *STATE.lock() }

// ❌ Wrong
pub static STATE: Mutex<MouseState> = ...;
```

Use `AtomicBool` / `AtomicU64` instead of `Mutex<bool>` / `Mutex<u64>`:

```rust
// ✅ Correct
static INITIALIZED: AtomicBool = AtomicBool::new(false);

// ❌ Wrong
static INITIALIZED: Mutex<bool> = Mutex::new(false);
```

---

## 5. Unsafe Rules

Minimize `unsafe` block size. Every `unsafe` block MUST have a `// SAFETY:` comment.

```rust
// ✅ Correct
let ptr = frame_allocator.allocate_frame().expect("OOM: PML4 frame");
// SAFETY: ptr is a valid 4KiB-aligned physical address returned by
//         BumpFrameAllocator, guaranteed non-zero and exclusively owned.
let table = unsafe { &mut *(ptr as *mut PageTable) };

// ❌ Wrong: large unsafe block with no SAFETY comment
unsafe {
    let ptr = allocate();
    let table = &mut *(ptr as *mut PageTable);
    // ... 30 lines of logic ...
}
```

---

## 6. Documentation

All `pub` functions, structs, and enums MUST have `///` doc comments.

```rust
// ✅ Correct
/// A decoded PS/2 mouse packet.
pub struct MousePacket { ... }

/// Parse three raw PS/2 bytes into a [`MousePacket`].
///
/// Returns `None` if the sync bit (bit 3 of byte 0) is not set.
pub fn parse(b0: u8, b1: u8, b2: u8) -> Option<Self> { ... }

// ❌ Wrong: no doc comment on pub item
pub struct MousePacket { ... }
```

---

## 7. Interrupt Handler Rules

Interrupt handlers must do the MINIMUM possible work:

```rust
// ✅ Correct: push to queue and return
extern "x86-interrupt" fn mouse_interrupt_handler(_: InterruptStackFrame) {
    let byte = unsafe { Port::<u8>::new(0x60).read() };
    call_mouse_byte_processor(byte);
    // SAFETY: PIC EOI must be sent after every hardware interrupt.
    unsafe { send_eoi(InterruptIndex::Mouse) };
}

// ❌ Wrong: packet parsing inside interrupt handler
extern "x86-interrupt" fn mouse_interrupt_handler(_: InterruptStackFrame) {
    let byte = unsafe { Port::<u8>::new(0x60).read() };
    // ... parsing logic, state updates ... ← NEVER do this
}
```

---

## 8. Error Handling

```rust
// Boot phase (UEFI available): expect() is acceptable
let handle = st.boot_services()
    .get_handle_for_protocol::<GraphicsOutput>()
    .expect("GOP not found: UEFI GOP is required for ManaOS");

// Kernel phase: include context in the panic message
let frame = frame_allocator
    .allocate_frames(pages)
    .unwrap_or_else(|| panic!("OOM: failed to allocate {} pages for heap", pages));
```

---

## 9. Checklist Before Suggesting Any Code

Before writing or suggesting any code, verify:

- [ ] File is in the correct directory per Section 1
- [ ] Names are clear per Section 2
- [ ] `mod.rs` has a module-level doc comment per Section 3
- [ ] No `pub static` per Section 4
- [ ] Every `unsafe` has a `// SAFETY:` comment per Section 5
- [ ] All `pub` items have `///` doc comments per Section 6
- [ ] Interrupt handlers only push to queue per Section 7
- [ ] `expect()` messages are descriptive per Section 8

## 10. Documentation Rules

Use Rust `///` doc comments. Never use JSDoc style.

### Required on ALL `pub` items

- Functions: one-line summary + `# Panics` / `# Safety` sections if applicable
- Structs: summary + each `pub` field documented
- Enums: summary + each variant documented
- Modules: `//!` at the top of every `mod.rs`

### `// SAFETY:` on every `unsafe` block (inline, not as doc comment)

### What NOT to document

- Private functions that are obvious from their name
- Re-exports in mod.rs (`pub use` lines don't need comments)

### Clippy enforces this

`#![deny(missing_docs)]` is set in main.rs — missing doc = compile error.

---

## 11. Git Workflow

Agents must work in a task branch, verify the branch, merge it into `master`,
push `origin/master`, and delete the task branch after success.

1. Start from a clean `master` unless the project owner has intentionally left
   working-tree changes to include.
2. Create a focused branch such as `feature/...`, `fix/...`, `refactor/...`, or
   `docs/...`.
3. Commit with a clear English message that follows the commit rules below.
4. Run the relevant checks before merging. For Rust code, run `cargo fmt`,
   `cargo check`, and `cargo clippy --all-targets --all-features`; run
   `just lint` when the change touches kernel/userland boundaries.
5. Merge the verified branch into `master`, push `origin/master`, then delete
   the local and remote task branch.

---

## 12. Commit Message And History Rules

Commit history is part of the maintainer workflow. Agents must keep it readable
and recoverable.

### Commit Message Format

- Use English only.
- Use the `type(scope): summary` subject format, for example
  `docs(process): define execve lifecycle contract`.
- Use a lowercase change type such as `feat`, `fix`, `docs`, `refactor`,
  `test`, `chore`, or `build`.
- Use a short lowercase scope that names the touched subsystem, document group,
  or workflow area, such as `process`, `memory`, `storage`, `agents`, or `ci`.
- Keep the subject focused on the change, not on the tool or agent that made it.
- For non-trivial changes, include a short body with the reason and validation
  commands.
- Do not mention skipped checks unless the reason is specific and actionable.

### Commit Scope

- Prefer one verified commit per focused task branch.
- Do not mix unrelated code, documentation, generated metadata, and formatting
  churn in the same commit.
- If generated files change, mention the generator command in the commit body
  or handoff notes.
- If a commit updates `TODO.md`, keep the completed/unfinished split consistent
  in the same commit.

### History Cleanup

- Do not rewrite `master` history unless the project owner explicitly requests
  it.
- Before rewriting `master`, create a backup branch that points to the exact
  pre-rewrite tip.
- Prefer non-interactive history cleanup commands with an explicit base commit.
- Use `--force-with-lease` rather than an unconditional force push when pushing
  rewritten `master`.
- After cleanup, verify that `master` is clean, the backup branch exists, and
  the intended commit range was rewritten.

---

## 13. Markdown And Documentation Maintenance

Documentation is part of the project contract. Agents must keep Markdown files
accurate, navigable, and synchronized with implementation reality.

### English Source Of Truth

- English Markdown files are the source of truth.
- Japanese files are companion documents for discussion and onboarding.
- When adding or changing a contributor-facing document under `docs/`, add or
  update the matching `docs/ja/*.ja.md` file unless the project owner explicitly
  says not to.
- Do not create Japanese companion files for agent-only instruction files unless
  the project owner explicitly requests them.

### README And Documentation Map

- If a new contributor-facing Markdown file is added, update the documentation
  map in `README.md` and `docs/ja/README.ja.md`.
- Keep README topic guidance concrete: tell readers which document to read for
  architecture, memory, syscall, storage, scheduler, tooling, and TODO work.
- Prefer precise ownership, invariants, failure modes, and validation commands
  over broad marketing language.

### TODO Files

- `TODO.md` must list unfinished work only.
- Completed TODO items must move to `TODO_COMPLETED.md` after the implementing
  branch is verified.
- `docs/ja/TODO.ja.md` is a Japanese guide to the active roadmap, not a stale
  duplicate checklist.
- `docs/ja/TODO_COMPLETED.ja.md` is a Japanese guide to the completed archive,
  not the authoritative item-by-item record.

### Generated And External Metadata

- Do not hand-edit generated Markdown such as `THIRD_PARTY_LICENSES.md`.
- Refresh generated license metadata with `just licenses` after dependency
  changes.
- If generated content needs explanation, add it to a separate guide file rather
  than modifying the generated table by hand.

### Documentation Verification

- For docs-only changes, run `git diff --check` before committing.
- After committing docs-only changes, run `git show --check --stat --oneline
  HEAD` or an equivalent staged/commit whitespace check.
- Check local Markdown links when changing README files, documentation maps, or
  file names.
- Rust build, lint, and QEMU smoke checks are not required for docs-only changes
  unless the docs change accompanies behavior changes.
