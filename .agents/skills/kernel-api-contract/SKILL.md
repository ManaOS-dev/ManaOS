---
name: kernel-api-contract
description: Preserve ManaOS kernel API contracts, module boundaries, init order, and call-site invariants when changing public/internal APIs, traits, modules, init functions, globals, allocator, memory, scheduler/task, interrupt, syscall, or driver interfaces. Skip local implementation-only edits with no call-site impact.
---

# Kernel API Contract

## When To Use

Use this skill when changing public or internal APIs, traits, module interfaces, `init` or `initialize` functions, global state accessors, allocator APIs, memory APIs, scheduler/task APIs, interrupt APIs, syscall ABI, or driver interfaces. Do not use it for local implementation-only changes that do not affect call sites.

## Inputs To Inspect

- The changed API declaration and all call sites with `rg`.
- `AGENTS.md` module and naming rules.
- `docs/ARCHITECTURE.md` for composition-root and dependency direction.
- `docs/MEMORY_MANAGEMENT.md`, `docs/USER_TRAP_FRAME.md`, and `docs/USER_POINTER_VALIDATION.md` when memory, task, or syscall contracts change.
- `build.rs`, `src/shared/syscall_contract.rs`, and `userland/src/syscall/mod.rs` when syscall/userland ABI changes.

## Workflow

1. State the current contract: owner module, allowed callers, init preconditions, and lifetime/ownership expectations.
2. Search all call sites before editing.
3. Preserve dependency direction: `arch/` must not depend on `kernel/`; `main.rs` owns wiring; kernel subsystems should use registered providers or explicit abstractions.
4. Preserve `mod.rs` ownership docs and keep `mod.rs` thin.
5. Preserve public item docs and descriptive names.
6. For globals, keep statics private and expose state through functions.
7. For syscall/userland changes, update shared contract and userland wrapper together.
8. Add or update smoke/log assertions when the contract is runtime-observable.

## Repo-Specific Commands

- Search call sites: `rg "<api_name>" src userland build.rs docs`
- Format: `just fmt`
- Kernel check: `cargo check --target x86_64-unknown-uefi`
- Userland/kernel lint and architecture boundary: `just lint`
- Strict clippy: `cargo clippy --all-targets --all-features -- -D warnings`
- Runtime contract proof when boot-visible: `just storage-smoke`

## Safety Checks

- Check init order before using globals, allocators, loggers, paging, scheduler, interrupts, or drivers.
- Do not add callbacks that let `arch/` call `kernel::...` directly.
- Do not change syscall ABI numbers, struct layout, or trap-frame layout without matching docs, userland wrappers, and smoke evidence.
- Do not rely on successful compilation alone for API contract correctness in unsafe or concurrent paths.
- Keep changes minimal; avoid unrelated refactors and compatibility shims unless they serve a real migration need.

## Done Criteria

- All call sites compile against the new contract.
- Init order and ownership preconditions are documented or preserved.
- Boundary checks pass when boundaries are touched.
- Runtime-visible contracts are covered by existing or updated smoke checks.

## Report Back

Report the contract changed, call-site impact, initialization or ownership assumptions, checks run, and any recommendation that belongs in `AGENTS.md` rather than source.
