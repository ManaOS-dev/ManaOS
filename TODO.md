# ManaOS TODO

## Now (Current Sprint)
- [x] Fix timer EOI unconditional send (`timer_interrupt_handler` try-lock bug)
- [x] Clamp mouse cursor coordinates in `state.rs`, not only in `draw_cursor()`
- [x] Guard FPS division against zero in `runtime::tick()`

## Phase 5: Filesystem & Storage
- [x] Phase 5A: Kernel-side file abstraction
- [x] VFS abstraction layer
- [x] ramfs
- [x] `/dev/console`
- [x] `/dev/null`
- [x] FileDescriptor table
- [ ] Phase 5B: Userland I/O
- [x] Phase 5B-1: SYS_WRITE only
- [x] syscall ABI
- [x] SYS_WRITE implementation
- [x] temporary user pointer validation
- [x] userland write wrapper
- [x] `hello from userland` output
- [x] Phase 5B-2: SYS_EXIT plus one-shot user demo
- [x] SYS_EXIT implementation
- [x] mark current user task finished on exit
- [x] one-shot user demo runner
- [x] resume UI after user exit
- [ ] syscall read/open/close
- [ ] minimal shell-style task
- [ ] Phase 5C: Real Storage
- [ ] GPT partition table parsing
- [ ] AHCI Driver Implementation
- [ ] FAT32 Parser & File APIs

## Phase 6: Userland
- [ ] ELF Loader
- [ ] System Call API Definitions
- [ ] Shell Implementation
- [ ] Dynamic linker stub

## Phase 7: Kernel Hardening
- [ ] ACPI MADT parsing
- [ ] IOAPIC routing (replace legacy 8259 PIC)
- [ ] Local APIC timer (replace PIT)
- [ ] Save/restore full user trap frame on context switch
- [ ] Per-process virtual address space (separate page tables)
- [ ] Guard pages between kernel stacks
- [ ] Virtual memory allocator (for dynamic kernel mappings)
- [ ] Console text output with scroll
- [ ] Window / widget primitive layer

## Completed
<details>
<summary>Phase 1-4 (click to expand)</summary>

### Refactoring

- [x] Split boot-time memory/display initialization out of `main.rs`
- [x] Move main-loop tick processing out of `main.rs`
- [x] Remove direct `arch/` to `kernel/` calls from interrupt handlers
- [x] Wire interrupt callbacks from `main.rs`
- [x] Rework interrupt callback registration into a single `InterruptProcessors` registration API
- [x] Add `kernel::interrupt` bridge for kernel-side interrupt event routing
- [x] Fix stale boot memory map usage after boot-service pool allocations
- [x] Persist PS/2 mouse packet assembly state across `process_packets()` calls
- [x] Make display command processing non-dropping when the framebuffer lock is busy
- [x] Add missing `// SAFETY:` comments in remaining unsafe-heavy modules
- [x] Move cursor rendering ownership from input mouse code to display cursor code

### Phase 1: Memory Management & Foundation

- [x] Memory Map Acquisition & `ExitBootServices`
- [x] Physical Frame Allocator (Bump Allocator)
- [x] Heap Allocator (`linked_list_allocator`)
- [x] Architecture Separation (`arch/` layer established)
- [x] Explicit Paging Setup (Identity Mapping)
- [x] Rebuild or refresh allocator regions from the final memory map after all boot-service allocations

### Phase 2: Interrupts & Exceptions

- [x] GDT / IDT Setup (with Data Segments)
- [x] Exception Handlers (Page Fault, Double Fault, GPF)
- [x] Mouse Driver (PS/2) with Real-time Cursor, Lock-Free Async Queue & Dirty Rectangles
- [x] Keyboard Driver (PS/2) - Interrupt driven & Lock-Free Async Queue
- [x] Interrupt callback boundary: `arch/` dispatches to registered callbacks, not `kernel/`
- [x] Consolidate callback registration with `InterruptProcessors`
- [x] Add timeouts to PS/2 controller busy waits
- [x] Timer backend abstraction with Local APIC capability detection
- [x] Interrupt controller abstraction with IOAPIC routing boundary

### Phase 3: Graphics & Console

- [x] Serial Output (COM1)
- [x] GOP Framebuffer Control
- [x] Font Engine (`ab_glyph`)
- [x] Proper Alpha Blending for Text (Pixel-perfect rounding)
- [x] Double Buffering & Dirty Rectangles Optimization (1000fps ready)
- [x] RDTSC Profiling & Calibration
- [x] Split renderer/font/cursor responsibilities out of `framebuffer.rs`
- [x] Avoid dropping queued draw commands on temporary framebuffer lock contention

### Phase 4: Process Management

- [x] Task Structure & Context Switching
- [x] Cooperative / Preemptive Scheduler
- [x] Ring 3 descriptor groundwork and selector exposure
- [x] Enter user mode with `iretq` and a user stack
- [x] Minimal `SYSCALL`/`SYSRET` MSR setup and syscall bridge stub

</details>
