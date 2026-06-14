---
name: boot-qemu-debug-loop
description: Debug ManaOS build, linker, bootloader, QEMU, serial log, panic, hang, and kernel integration failures with a build -> boot -> observe logs -> first failing stage -> minimal fix loop. Use for boot/QEMU/panic/serial/emulator/linker failures or integration tests; skip static review when no commands should run.
---

# Boot QEMU Debug Loop

## When To Use

Use this skill when debugging boot failures, QEMU failures, panic output, serial logs, emulator hangs, linker/bootloader failures, storage smoke failures, or kernel integration tests. Do not use it for static code review when commands should not be run.

## Inputs To Inspect

- `justfile`, `run.bat`, `run.sh`, and `scripts/run_storage_smoke.ps1`.
- Latest `storage-smoke-serial.log` if present.
- `README.md` and `docs/MANUAL_QEMU_VALIDATION.md`.
- Recent diffs and touched boot, memory, interrupt, storage, filesystem, syscall, or scheduler code.
- QEMU/OVMF availability assumptions: `OVMF.fd`, `qemu-system-x86_64`, `disk.img`, `esp/EFI/BOOT/BOOTX64.EFI`.

## Workflow

1. Start with a clean understanding of the current branch and diff.
2. Build first; do not debug stale binaries.
3. Prefer the automated headless smoke path: `just storage-smoke`.
4. If smoke fails, inspect the first missing expected pattern and the last 80 serial log lines.
5. Identify the first failing boot stage: build, disk image, OVMF/UEFI, kernel entry, allocator/paging, ACPI/APIC, storage, filesystem, scheduler, syscall, userland, or console.
6. Make the smallest fix for that stage only.
7. Re-run the same failing command. Escalate to broader checks only after the first failure is resolved.
8. Use manual QEMU only when interactive console observation is required.

## Repo-Specific Commands

- Build kernel and userland through Cargo build script: `cargo build`
- Kernel UEFI target build: `cargo build --target x86_64-unknown-uefi`
- Headless boot and serial assertions: `just storage-smoke`
- Interactive run through just: `just` or `just run`
- Windows direct run: `run.bat`
- Linux/macOS direct run: `./run.sh`
- Manual validation guide: `docs/MANUAL_QEMU_VALIDATION.md`

## Safety Checks

- Do not run destructive cleanup commands while debugging.
- Note that `just storage-smoke` recreates/updates boot artifacts and disk image contents as part of the repo's test flow.
- Do not treat successful compilation or a successful boot as proof of unsafe/concurrency correctness.
- Do not patch multiple boot stages at once; preserve a reproducible failing command.
- Keep fixes minimal and avoid unrelated refactors.

## Done Criteria

- The failing command is identified.
- The first failing stage is named.
- A minimal fix is applied or a precise blocker is reported.
- The same command that failed now passes, or the remaining failure has a narrowed cause.

## Report Back

Report the command run, first failing stage, key serial log lines or missing patterns, the minimal change made, and the final boot/check result.
