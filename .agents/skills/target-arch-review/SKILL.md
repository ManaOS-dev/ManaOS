---
name: target-arch-review
description: Apply ManaOS x86_64 architecture-specific review for page tables, exception vectors, GDT/IDT, APIC/PIC, syscall/trap handling, context switching, boot assembly, linker/target files, and CPU/ABI assumptions. Use for architecture-specific code; skip architecture-independent docs or tests.
---

# Target Arch Review

## When To Use

Use this skill when editing architecture-specific code, page tables, exception vectors, GDT/IDT, APIC/PIC, syscall/trap handling, context switching, boot assembly, linker scripts, target configuration, or CPU/ABI assumptions. Do not use it for architecture-independent docs or tests.

## Inputs To Inspect

- `Cargo.toml` package description and target commands: ManaOS is a monolithic `x86_64` UEFI kernel.
- `src/arch/x86_64/mod.rs`, `global_descriptor_table.rs`, `interrupt_descriptor_table.rs`, `interrupt_controller.rs`, `interval_timer.rs`, `context_switch.s`, and `interrupt_entry.s`.
- `src/shared/timer_interrupt_frame.rs` and `src/kernel/task/context/*` for register layout contracts.
- `src/shared/syscall_contract.rs`, `src/kernel/syscall.rs`, and `userland/src/syscall/raw.rs` for syscall ABI.
- Docs: `docs/ARCHITECTURE.md`, `docs/ACPI.md`, `docs/USER_TRAP_FRAME.md`.

## Workflow

1. State the relevant target assumptions: x86_64, UEFI, `x86_64-unknown-uefi` kernel, `x86_64-unknown-none` userland, `abi_x86_interrupt`, `iretq`, `syscall`, GDT/IDT, APIC/IOAPIC/PIC, and current QEMU/OVMF boot.
2. Check assembly and Rust struct layouts together. Verify `repr(C)` and offset assertions when register frames change.
3. Check interrupt handlers remain tiny: read hardware state, acknowledge controller, dispatch registered callbacks only.
4. Check APIC/PIC routing and EOI behavior against `docs/ACPI.md`.
5. Check page-table and address-space changes for x86_64 permission bits, user/supervisor access, NX, CR3 switching, identity-map assumptions, and guard pages.
6. Check syscall/trap handling for register clobbers, selectors, flags masking, user stack/kernel stack switching, and return path.
7. Run architecture boundary checks when `arch/` or providers change.

## Repo-Specific Commands

- Kernel target check: `cargo check --target x86_64-unknown-uefi`
- Architecture boundary check: `just architecture-boundaries`
- Full lint including boundary check: `just lint`
- Boot and APIC/timer/syscall proof: `just storage-smoke`
- Interactive QEMU candidate when manual observation is needed: `just run` or `run.bat` / `./run.sh` based on host OS.

## Safety Checks

- Do not make `arch/` depend on `kernel/`; route through callbacks registered by `main.rs`.
- Do not block, allocate, or take ordinary locks in interrupt/exception paths unless existing code proves it is safe.
- Document CPU, ABI, register, and boot assumptions near unsafe or assembly-facing changes.
- Treat MMIO, volatile accesses, interrupt masking, and fences carefully.
- Do not treat successful compilation as proof of architecture correctness.
- Keep architecture changes minimal and avoid unrelated refactors.

## Done Criteria

- Target assumptions are explicit.
- Assembly/Rust layouts and ABI contracts are preserved or updated together.
- Architecture boundary checks pass for boundary-touching changes.
- Boot smoke or a justified narrower check covers the affected architecture path.

## Report Back

Report the target assumptions applied, architecture files reviewed, layout or ABI contracts checked, commands run, and any unresolved hardware/boot risk.
