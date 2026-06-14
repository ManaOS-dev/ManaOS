---
name: rust-os-safety-review
description: Review ManaOS kernel/OS unsafe code, raw pointers, volatile/MMIO, atomics, inline asm, globals, interrupt safety, allocator init, paging, syscalls, concurrency, and panic paths. Use for unsafe, boot, interrupts, memory, drivers, allocators, syscalls, or concurrency; skip ordinary README edits or planning with no code impact.
---

# Rust OS Safety Review

## When To Use

Use this skill when editing or reviewing kernel/OS code involving `unsafe`, raw pointers, volatile access, MMIO, memory ordering, inline assembly, global mutable state, interrupt/exception paths, allocator initialization, paging, drivers, syscalls, user pointers, or concurrency.

## Inputs To Inspect

- Touched code and direct callers/callees.
- `src/main.rs` init sequence for boot, allocator, paging, logging, drivers, scheduler, and interrupts.
- `src/arch/x86_64/*` for assembly, GDT/IDT, APIC/PIC, syscall/trap handling, and context switching.
- `src/kernel/memory/*`, `src/kernel/task/*`, `src/kernel/interrupt.rs`, `src/kernel/syscall.rs`, and relevant driver modules.
- Relevant docs: `docs/ARCHITECTURE.md`, `docs/MEMORY_MANAGEMENT.md`, `docs/USER_TRAP_FRAME.md`, `docs/USER_POINTER_VALIDATION.md`, `docs/KERNEL_STACKS.md`.

## Workflow

1. Identify every unsafe operation and the invariant it depends on.
2. Keep unsafe blocks small and place a concrete `// SAFETY:` comment directly nearby.
3. Check raw pointer alignment, lifetime, aliasing, provenance, page presence, and user/kernel address class.
4. Check volatile/MMIO access widths, register ordering, readback needs, and whether interrupt masking or fences are required.
5. Check atomics for ordering, publication, interrupt visibility, and whether `Relaxed` is actually sufficient.
6. Check interrupt/exception/syscall paths for blocking, allocation, ordinary locks, logging, or reentrancy hazards.
7. Check allocator and global initialization order before first use.
8. Check paging, address-space, and user pointer code for permission, NX/user/writable bits, guard pages, and cleanup ownership.
9. Treat successful compilation as necessary but not sufficient for safety-sensitive code.

## Repo-Specific Commands

- `cargo check --target x86_64-unknown-uefi`
- `just lint`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `just storage-smoke` for boot, interrupt, storage, scheduler, memory, syscall, or userland behavior.

## Safety Checks

- Do not block, allocate, or take ordinary locks in interrupt/exception paths unless existing code proves the path is safe.
- Do not dereference user pointers without the repository's user pointer validation policy.
- Do not add public statics; expose state through functions.
- Do not assume MMIO behaves like normal memory.
- Do not assume compile success proves correctness for unsafe, kernel, or concurrency-sensitive edits.
- Avoid unrelated refactors while fixing safety issues.

## Done Criteria

- Each unsafe block has a local, specific invariant.
- Interrupt and syscall paths remain minimal and nonblocking.
- Memory ownership, paging permissions, and cleanup paths are accounted for.
- The review identifies residual risks and the first failing stage if behavior was tested.

## Report Back

Report reviewed files, safety invariants checked, commands run, any findings by severity, and remaining risks or missing runtime coverage.
