# ManaOS

**[ManaOS](https://discord.gg/FXTV344M94)** is a monolithic x86_64 UEFI kernel developed in Rust, designed with a focus on scalability and contributor friendliness—truly an **"OS for Developers."**

## 🚀 Key Features

- **HAL Architecture**: Strict separation between hardware-specific and generic kernel logic.
- **Callback-Based Interrupt Wiring**: `arch/` handles hardware interrupts without directly depending on `kernel/`.
- **Boot/Runtime Split**: `main.rs` wires the system together while boot and runtime modules own focused initialization and tick processing.
- **Developer-First API Direction**: Ergonomic APIs with Rust-style naming,
  such as future drawing helpers like `graphics.draw_text`.
- **Global Collaboration**: English-first documentation and standard PR-based workflow.
- **Modern Tooling**: Seamless build and run experience with `just` and `qemu`.

## 🛠 Getting Started

### Prerequisites

- [Rust (Nightly)](https://rustup.rs/)
- [QEMU](https://www.qemu.org/)
- `OVMF.fd` (Place it in the root directory)
- [`just`](https://github.com/casey/just) for the documented build, run, lint,
  and smoke-test commands

### Build and Run

If you have `just` installed:

```bash
just
```

Alternatively, use the provided scripts:

- **Windows**: `run.bat`
- **Linux/macOS**: `./run.sh`

## Project Topics

ManaOS is organized around a small set of engineering topics. Use these topics
to decide which document to read before changing code:

- **Architecture Boundaries**: `main.rs` is the composition root, `arch/` owns
  hardware-specific entry points, and `kernel/` owns platform-independent
  policy.
- **Interrupts And Timers**: hardware interrupt handlers stay minimal, dispatch
  through registered callbacks, and acknowledge the active interrupt-controller
  backend.
- **Memory Ownership**: physical frames, user address spaces, kernel virtual
  mappings, DMA buffers, and guarded stacks each have explicit owner rules.
- **User Processes**: ELF loading, user stacks, syscall entry, trap frames,
  preemption, process metadata, and future `execve` / `waitpid` work are tracked
  as one lifecycle.
- **Storage And Filesystems**: AHCI, GPT, FAT32, VFS, path normalization, file
  descriptors, and future write support are treated as separate layers.
- **Developer Workflow**: contributors use pull requests; maintainers may use
  the direct branch workflow only after local verification.

## Repository Layout

The repository is intentionally split by ownership boundary:

| Path | Purpose |
| --- | --- |
| `src/main.rs` | Composition root. Wires boot data, architecture providers, kernel services, drivers, and smoke lifecycle. |
| `src/arch/` | Architecture-specific CPU, interrupt, APIC/PIC, syscall, context switch, and MMIO code. Must not depend on `kernel/`. |
| `src/kernel/` | Platform-independent kernel policy: memory, task scheduling, syscalls, filesystems, drivers, diagnostics, and console services. |
| `src/shared/` | ABI contracts and small shared layouts used across kernel and userland-facing boundaries. |
| `userland/` | no-std userland runtime and demo programs built for `x86_64-unknown-none`. |
| `docs/` | English design documents and validation guides. |
| `docs/ja/` | Japanese companion documents for discussion and onboarding. |
| `scripts/` | Host-side PowerShell tooling for disk images, smoke runs, license generation, and boundary checks. |
| `templates/` | Copy-start templates for modules, drivers, design docs, and commit messages. |
| `esp/` | UEFI system partition assets copied into the QEMU boot image. |

Before changing a subsystem, read the nearest design document and the module's
`mod.rs` ownership comment. If an edit crosses a boundary, update the
documentation in the same branch.

## What To Read First

| Change area | Start here |
| --- | --- |
| New agent or automated change | [AGENTS.md](AGENTS.md), then [CONTRIBUTING.md](CONTRIBUTING.md) |
| Architecture, interrupt, or syscall entry work | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/USER_TRAP_FRAME.md](docs/USER_TRAP_FRAME.md) |
| ACPI, APIC, IOAPIC, PIC, or timer work | [docs/ACPI.md](docs/ACPI.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Memory, paging, stacks, or address ownership | [docs/MEMORY_MANAGEMENT.md](docs/MEMORY_MANAGEMENT.md), [docs/ADDRESS_BOUNDARIES.md](docs/ADDRESS_BOUNDARIES.md), [docs/KERNEL_STACKS.md](docs/KERNEL_STACKS.md) |
| Syscall pointer validation | [docs/USER_POINTER_VALIDATION.md](docs/USER_POINTER_VALIDATION.md), [docs/USER_TRAP_FRAME.md](docs/USER_TRAP_FRAME.md) |
| Storage, FAT32, VFS, or console file commands | [docs/FILESYSTEM.md](docs/FILESYSTEM.md), [docs/MANUAL_QEMU_VALIDATION.md](docs/MANUAL_QEMU_VALIDATION.md) |
| Choosing the next task | [TODO.md](TODO.md), [docs/TASK_PRIORITY.md](docs/TASK_PRIORITY.md) |

## Documentation Map

English documents are the source of truth. Japanese companion documents are kept
for smoother discussion and onboarding.

| Topic | English | Japanese |
| --- | --- | --- |
| Project overview | [README.md](README.md) | [docs/ja/README.ja.md](docs/ja/README.ja.md) |
| Contribution rules | [CONTRIBUTING.md](CONTRIBUTING.md) | [docs/ja/CONTRIBUTING.ja.md](docs/ja/CONTRIBUTING.ja.md) |
| Agent rules | [AGENTS.md](AGENTS.md) | English source only |
| Maintainer workflow | [docs/MAINTAINER_WORKFLOW.md](docs/MAINTAINER_WORKFLOW.md) | [docs/ja/MAINTAINER_WORKFLOW.ja.md](docs/ja/MAINTAINER_WORKFLOW.ja.md) |
| Architecture | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | [docs/ja/ARCHITECTURE.ja.md](docs/ja/ARCHITECTURE.ja.md) |
| ACPI and APIC | [docs/ACPI.md](docs/ACPI.md) | [docs/ja/ACPI.ja.md](docs/ja/ACPI.ja.md) |
| Address boundaries | [docs/ADDRESS_BOUNDARIES.md](docs/ADDRESS_BOUNDARIES.md) | [docs/ja/ADDRESS_BOUNDARIES.ja.md](docs/ja/ADDRESS_BOUNDARIES.ja.md) |
| Memory management | [docs/MEMORY_MANAGEMENT.md](docs/MEMORY_MANAGEMENT.md) | [docs/ja/MEMORY_MANAGEMENT.ja.md](docs/ja/MEMORY_MANAGEMENT.ja.md) |
| Kernel stacks | [docs/KERNEL_STACKS.md](docs/KERNEL_STACKS.md) | [docs/ja/KERNEL_STACKS.ja.md](docs/ja/KERNEL_STACKS.ja.md) |
| User trap frames | [docs/USER_TRAP_FRAME.md](docs/USER_TRAP_FRAME.md) | [docs/ja/USER_TRAP_FRAME.ja.md](docs/ja/USER_TRAP_FRAME.ja.md) |
| User pointer validation | [docs/USER_POINTER_VALIDATION.md](docs/USER_POINTER_VALIDATION.md) | [docs/ja/USER_POINTER_VALIDATION.ja.md](docs/ja/USER_POINTER_VALIDATION.ja.md) |
| Filesystem | [docs/FILESYSTEM.md](docs/FILESYSTEM.md) | [docs/ja/FILESYSTEM.ja.md](docs/ja/FILESYSTEM.ja.md) |
| Manual QEMU validation | [docs/MANUAL_QEMU_VALIDATION.md](docs/MANUAL_QEMU_VALIDATION.md) | [docs/ja/MANUAL_QEMU_VALIDATION.ja.md](docs/ja/MANUAL_QEMU_VALIDATION.ja.md) |
| Task priority | [docs/TASK_PRIORITY.md](docs/TASK_PRIORITY.md) | [docs/ja/TASK_PRIORITY.ja.md](docs/ja/TASK_PRIORITY.ja.md) |
| Active TODO | [TODO.md](TODO.md) | [TODO.ja.md](TODO.ja.md) |
| Completed TODO archive | [TODO_COMPLETED.md](TODO_COMPLETED.md) | [TODO_COMPLETED.ja.md](TODO_COMPLETED.ja.md) |
| Templates | [templates/README.md](templates/README.md) | [templates/README.ja.md](templates/README.ja.md) |
| Security policy | [SECURITY.md](SECURITY.md) | [SECURITY.ja.md](SECURITY.ja.md) |
| Third-party licenses | [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) | [THIRD_PARTY_LICENSES.ja.md](THIRD_PARTY_LICENSES.ja.md) |

## Validation Quick Reference

Use the narrowest useful command first, then expand when a change crosses a
kernel boundary:

```bash
just fmt
cargo check
cargo check --target x86_64-unknown-uefi
just lint
just storage-smoke
```

- Use `git diff --check` or `git show --check` for documentation-only changes.
- Use `just lint` when architecture, kernel/userland, or syscall boundaries are
  touched.
- Use `just storage-smoke` for boot-visible changes, storage/filesystem work,
  scheduler changes, memory ownership changes, syscall behavior, or userland
  runtime behavior.

When a check is skipped, record the reason in the commit or handoff notes. A
successful compile is necessary, but not sufficient, for unsafe, interrupt,
memory, scheduler, or syscall changes.

## 🤝 Contributing

We welcome contributors from all over the world! Please check our **[CONTRIBUTING.md](CONTRIBUTING.md)** for guidelines on coding standards, language policy, design principles, and our roadmap.

For architecture and module ownership details, see **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)**.
For the current roadmap and known refactoring tasks, see **[TODO.md](TODO.md)**.
For Japanese documentation, start at **[docs/ja/README.ja.md](docs/ja/README.ja.md)**.

---

## 🇯🇵 日本語ドキュメント (Japanese Documentation)

日本語のドキュメントは **[docs/ja/README.ja.md](docs/ja/README.ja.md)** に用意されています。

---

## 📄 License

See the [LICENSE](LICENSE) file for the current project license.

---

Built with ❤️ for the developer community.
