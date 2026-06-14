---
name: no-std-rust-check
description: Guard ManaOS Rust no_std kernel and no_std userland edits against accidental std usage, host assumptions, dependency feature drift, allocator misuse, and ordinary Rust runtime assumptions. Use for Rust, Cargo, build.rs, features, dependencies, panic, alloc, logging, tests, or examples; skip host tooling that intentionally uses std.
---

# No-Std Rust Check

## When To Use

Use this skill when editing Rust code, `Cargo.toml`, `build.rs`, feature flags, dependencies, panic handlers, allocator usage, formatting, logging, tests, examples, or userland crates. Do not use it for host-side tooling that intentionally uses `std`, such as `build.rs` or PowerShell scripts, except to verify it does not leak host assumptions into kernel/userland crates.

## Inputs To Inspect

- `Cargo.toml` and `userland/Cargo.toml`.
- `src/main.rs` for `#![no_std]`, `extern crate alloc`, panic handler, and boot init order.
- `build.rs` for the `x86_64-unknown-none` userland build and `llvm-objcopy` extraction.
- Any touched dependency features, logging, allocation, formatting, tests, or examples.
- Nearby imports for `core`, `alloc`, `std`, and default feature usage.

## Workflow

1. Classify the edited code as kernel no_std, userland no_std, or host tooling.
2. For kernel/userland, prefer `core` and `alloc` explicitly; do not introduce `std`.
3. Check dependency feature changes for hidden `std` or host-only defaults. Use `default-features = false` when needed.
4. Prove allocator availability before adding `Vec`, `String`, `Box`, formatting allocation, or heap-backed logging.
5. Check panic and allocation failure paths. They must not assume unwinding.
6. Check tests/examples: kernel binaries have `test = false` and `bench = false`; userland targets `x86_64-unknown-none`.
7. Run targeted checks, then broader checks if dependency or feature behavior changed.

## Repo-Specific Commands

- Kernel build/check target: `cargo check --target x86_64-unknown-uefi`
- Userland lint target from `justfile`: `cargo clippy --manifest-path userland/Cargo.toml --target x86_64-unknown-none --target-dir target/userland --lib --bin file_demo --bin bad_pointer_demo --bin smoke_demo --bin user_shell -- -D warnings`
- Full lint: `just lint`
- Strict clippy: `cargo clippy --all-targets --all-features -- -D warnings`
- Boot proof when runtime behavior changes: `just storage-smoke`

## Safety Checks

- Do not add `std` imports in `src/` or `userland/src/`.
- Do not allocate before kernel/userland allocator initialization is proven for that execution context.
- Do not add formatting/logging that allocates in panic, interrupt, exception, or early boot paths unless existing code proves it is safe.
- Do not rely on successful compilation as proof for unsafe, allocator, initialization-order, or concurrency-sensitive correctness.
- Avoid unrelated refactors and keep dependency changes explicit.

## Done Criteria

- Kernel/userland code remains no_std-compatible.
- Alloc usage has a proven initialized allocator in the execution context.
- Feature and dependency changes do not pull in ordinary host runtime assumptions.
- Relevant checks pass or blockers are reported with exact commands and errors.

## Report Back

Report whether the touched code is kernel no_std, userland no_std, or host tooling; any dependency or allocator risks found; and the commands used to verify the result.
