# ManaOS — Agent Coding Rules

This file defines **mandatory rules** for all AI assistants contributing to ManaOS.
Do not deviate from these rules unless explicitly instructed by the project owner.

---

## 1. Directory Structure

```
src/
├── arch/
│   └── x86_64/
│       ├── mod.rs
│       ├── global_descriptor_table.rs
│       ├── interrupt_descriptor_table.rs
│       ├── interrupt_controller.rs
│       └── interval_timer.rs
└── kernel/
    ├── driver/
    │   ├── display/
    │   │   ├── mod.rs
    │   │   ├── framebuffer.rs
    │   │   ├── renderer.rs
    │   │   ├── font.rs
    │   │   └── cursor.rs
    │   └── input/
    │       ├── keyboard/
    │       │   ├── mod.rs
    │       │   ├── scancode.rs
    │       │   └── decoder.rs
    │       └── mouse/
    │           ├── mod.rs
    │           ├── packet.rs
    │           └── state.rs
    ├── memory/
    │   ├── frame_allocator.rs
    │   ├── heap.rs
    │   └── paging.rs
    ├── sync/
    │   └── ring_buffer.rs
    ├── task/
    │   ├── mod.rs
    │   └── context.rs
    └── profiler.rs
```

---

## 2. Naming Rules

### ❌ Abbreviations are BANNED

| Banned                            | Required                                                |
| --------------------------------- | ------------------------------------------------------- |
| `pic`, `pit`, `gop`, `idt`, `gdt` | Full concept name (see directory structure)             |
| `mem/`                            | `memory/`                                               |
| `fb_info`                         | `framebuffer_info`                                      |
| `let (h, v) = ...`                | `let (width, height) = ...`                             |
| `handle_mouse()`                  | `process_packets()`                                     |
| `handle_keyboard()`               | `process_input()`                                       |
| `mod_impl`                        | A descriptive name (e.g., `state`, `packet`, `decoder`) |

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
    crate::kernel::driver::input::mouse::push_byte(byte);
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
- [ ] No abbreviations per Section 2
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
