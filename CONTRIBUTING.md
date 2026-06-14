# Contributing to ManaOS

Welcome! ManaOS is an "OS for Developers," and we value your contributions. This document provides guidelines for participating in the project.

## 🤝 Contribution Workflow

External contributors should use a pull request workflow. When there are no
external contributors involved in the change, maintainers and project-owned
automation may use the direct branch workflow required by `AGENTS.md` and
documented in [`docs/MAINTAINER_WORKFLOW.md`](docs/MAINTAINER_WORKFLOW.md)
after local verification.

1. **Fork** the repository.
2. **Create a branch** for your feature or bug fix: `git checkout -b feature/your-awesome-feature` or `git checkout -b fix/your-bug`.
3. **Commit** your changes with clear messages.
4. **Format & Lint** your code (see below).
5. **Push** to your fork and **Open a Pull Request** targeting `master`.

## 🌿 Branch Policy

| Branch | Purpose |
|---|---|
| `master` | Always builds and boots with all merged work verified |
| `feature/xxx` | Single feature unit |
| `fix/xxx` | Bug fix |
| `refactor/xxx` | Code restructuring without behavior changes unless stated |
| `docs/xxx` | Documentation-only work |
| `experimental/xxx` | Experimental work; do not merge until converted to a verified branch |

- Pull requests from `feature/xxx`, `fix/xxx`, `refactor/xxx`, and `docs/xxx`
  target `master`.
- Keep each branch focused on one reviewable unit.
- Delete task branches after they are merged.

---

## 🇯🇵 日本語版 (Japanese Version)

日本語のガイドラインは **[docs/ja/CONTRIBUTING.ja.md](docs/ja/CONTRIBUTING.ja.md)** をご覧ください。

---

## 📝 Language Policy

- **Code & Comments**: **English only** (for global collaboration and better tool integration).
- **Commit Messages**: **English**. Use a concise imperative summary.
  Conventional Commit prefixes are optional when they add useful context.
- **Discussions**: **Japanese is welcome** in GitHub Issues and Pull Request comments to facilitate smooth and fast communication among core members.

## 🏹 Commit Message Convention

Use clear English commit messages. Conventional Commit prefixes are allowed but
not required:
- `feat: ...` (new feature)
- `fix: ...` (bug fix)
- `docs: ...` (documentation)
- `style: ...` (formatting, missing semi colons, etc)
- `refactor: ...` (code restructuring)
- `chore: ...` (maintenance)

## 🛠 Coding Standards

To maintain high code quality and consistency, please follow these rules:

### 1. Code Formatting
All code must be formatted using `rustfmt`. Run the following command before committing:
```bash
just fmt
```

### 2. Static Analysis
We use `clippy` to catch common mistakes. Your PR must pass clippy checks with no warnings:
```bash
just lint
```

### 3. Documentation
- All `pub` functions, structs, and enums must have `///` doc comments.
- Use English for all comments and documentation to ensure global accessibility.

### 4. Naming
- Avoid unclear local abbreviations such as `fb_info`, `h`, and `v`.
- Domain-standard acronyms are allowed when they improve readability, including
  `PCI`, `AHCI`, `GPT`, `FAT32`, `UEFI`, `GDT`, `IDT`, `GOP`, `PIC`, `PIT`,
  `APIC`, `IOAPIC`, `LBA`, `FIS`, `DMA`, and `PRDT`.
- Prefer concise acronyms in log categories and diagnostic messages.

### 5. Module Boundaries
- Keep `mod.rs` files thin: ownership documentation, module declarations,
  re-exports, and small public API forwarding only.
- Move processing logic into focused sibling modules such as `queue`, `decoder`,
  `state`, or `hardware`.

### 6. Safety
- Minimize the use of `unsafe` blocks.
- If you use `unsafe`, you **must** add a `// SAFETY:` comment explaining why it is safe.

## 📚 Documentation Standards

Documentation changes should be treated as part of the engineering contract, not
as an afterthought.

- English documents are the source of truth.
- Japanese companion documents should explain the same operational meaning when
  a Japanese file exists for the English document.
- Keep generated files generated. Do not hand-edit
  `THIRD_PARTY_LICENSES.md`; regenerate it with `just licenses`.
- Keep `TODO.md` limited to unfinished work. Move completed items into
  `TODO_COMPLETED.md` after the implementing branch is verified.
- When changing architecture, memory, syscall, storage, scheduler, or userland
  behavior, update the nearest design document in the same branch.
- When adding a new Markdown file under `docs/`, add or deliberately skip a
  Japanese companion and update the documentation map in `README.md` if it is a
  contributor-facing document.
- Prefer concrete invariants, ownership rules, failure modes, and validation
  commands over vague roadmap text.

## ✅ Verification Matrix

Use the smallest meaningful check first, then broaden when the change crosses
runtime boundaries.

| Change type | Minimum verification |
| --- | --- |
| Documentation only | `git diff --check` or `git show --check` |
| Formatting-only Rust changes | `just fmt` |
| Kernel Rust behavior | `cargo check --target x86_64-unknown-uefi` |
| Userland no-std behavior | `cargo clippy --manifest-path userland/Cargo.toml --target x86_64-unknown-none --target-dir target/userland --lib --bin file_demo --bin bad_pointer_demo --bin smoke_demo -- -D warnings` |
| Architecture or kernel/userland boundary | `just lint` |
| Boot-visible runtime behavior | `just storage-smoke` |

If a command cannot be run locally, record the exact reason and the expected
follow-up validation.

---

## 🛠 Design Principles (Scalable & Contributor Friendly)

### 1. HAL (Hardware Abstraction Layer)
Strictly separate architecture-dependent code from generic logic to support future architectures (e.g., AArch64).
- **`kernel/`**: Platform-independent logic (scheduler, filesystem, network stack, etc.).
- **`arch/x86_64/`**: CPU-specific implementations (GDT, IDT, page table manipulation, context switching, etc.).
- **Interface**: Kernel core interacts only through abstraction APIs provided by the `arch::` module.
- **Interrupt Boundary**: `arch/` must not call `kernel::...` directly. Interrupt handlers dispatch to callbacks registered by `main.rs`.

### 2. Trait-Driven Driver Design
Abstract device drivers using traits to allow modular expansion.
- **Console Trait**: Treat Serial, GOP, etc., as common Write operations.
- **BlockDevice Trait**: Abstract disk access for AHCI, NVMe, etc.

### 3. Type-Safe Memory Management (Newtype Pattern)
Distinguish between physical and virtual addresses at the type level to prevent bugs.
- Strict separation using `PhysAddr(u64)` and `VirtAddr(u64)`.
- Every `unsafe` block must be accompanied by a `// SAFETY:` comment and isolated in minimal modules.

### 4. Developer Experience (DX) & Quality
- **Standardized Tooling**: One-command build/run/test using `just`.
- **Code Consistency**: All code must follow `rustfmt` rules (`just fmt`).
- **Static Analysis**: Strictly enforced `cargo clippy` checks (`just lint`).
- **Documentation**: All `pub` items must have `///` doc comments.
- **Auto Documentation**: Visualize internal structures with `cargo doc`.

---

---

## 📅 Roadmap & TODOs

Please refer to **[TODO.md](TODO.md)** for the current project status and future roadmap.
For module ownership and interrupt wiring details, see **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)**.
