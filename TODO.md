# ManaOS TODO

## Current Refactoring Focus
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

## Phase 1: Memory Management & Foundation
- [x] Memory Map Acquisition & `ExitBootServices`
- [x] Physical Frame Allocator (Bump Allocator)
- [x] Heap Allocator (`linked_list_allocator`)
- [x] Architecture Separation (`arch/` layer established)
- [x] Explicit Paging Setup (Identity Mapping)
- [x] Rebuild or refresh allocator regions from the final memory map after all boot-service allocations

## Phase 2: Interrupts & Exceptions
- [x] GDT / IDT Setup (with Data Segments)
- [x] Exception Handlers (Page Fault, Double Fault, GPF)
- [x] Mouse Driver (PS/2) with Real-time Cursor, Lock-Free Async Queue & Dirty Rectangles
- [x] Keyboard Driver (PS/2) - Interrupt driven & Lock-Free Async Queue
- [x] Interrupt callback boundary: `arch/` dispatches to registered callbacks, not `kernel/`
- [x] Consolidate callback registration with `InterruptProcessors`
- [x] Add timeouts to PS/2 controller busy waits
- [x] Timer backend abstraction with Local APIC capability detection
- [x] Interrupt controller abstraction with IOAPIC routing boundary

## Phase 3: Graphics & Console
- [x] Serial Output (COM1)
- [x] GOP Framebuffer Control
- [x] Font Engine (`ab_glyph`)
- [x] Proper Alpha Blending for Text (Pixel-perfect rounding)
- [x] Double Buffering & Dirty Rectangles Optimization (1000fps ready)
- [x] RDTSC Profiling & Calibration
- [x] Split renderer/font/cursor responsibilities out of `framebuffer.rs`
- [x] Avoid dropping queued draw commands on temporary framebuffer lock contention

## Phase 4: Process Management
- [x] Task Structure & Context Switching
- [x] Cooperative / Preemptive Scheduler
- [x] Ring 3 descriptor groundwork and selector exposure
- [x] Enter user mode with `iretq` and a user stack
- [x] Minimal `SYSCALL`/`SYSRET` MSR setup and syscall bridge stub

## Phase 5: Filesystem & Storage
- [ ] AHCI Driver Implementation
- [ ] FAT32 Parser & File APIs

## Phase 6: Userland
- [ ] ELF Loader
- [ ] System Call API Definitions
- [ ] Shell Implementation
