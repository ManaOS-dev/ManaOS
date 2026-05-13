# Contributing to ManaOS

Welcome! ManaOS is an "OS for Developers," and we value your contributions. This document provides guidelines for participating in the project.

## 🤝 Pull Request Workflow

We follow a **Pull Request (PR) first** development model.

1. **Fork** the repository.
2. **Create a branch** for your feature or bug fix: `git checkout -b feature/your-awesome-feature` or `git checkout -b fix/your-bug`.
3. **Commit** your changes with clear messages.
4. **Format & Lint** your code (see below).
5. **Push** to your fork and **Open a Pull Request**.

## 🌿 Branch Policy

| Branch | Purpose |
|---|---|
| `main` | Always builds and boots with all features fully working |
| `dev` | Integration branch for non-experimental work |
| `feature/xxx` | Single feature unit |
| `fix/xxx` | Bug fix |
| `experimental/xxx` | Experimental work — breaking changes allowed |

- PRs from `feature/xxx` and `fix/xxx` target `dev`.
- `dev` is merged into `main` only when fully verified.
- `experimental/xxx` branches are never merged into `main` directly.

---

## 🇯🇵 日本語版 (Japanese Version)

日本語のガイドラインは **[docs/ja/CONTRIBUTING.ja.md](docs/ja/CONTRIBUTING.ja.md)** をご覧ください。

---

## 📝 Language Policy

- **Code & Comments**: **English only** (for global collaboration and better tool integration).
- **Commit Messages**: **English** (following [Conventional Commits](https://www.conventionalcommits.org/)).
- **Discussions**: **Japanese is welcome** in GitHub Issues and Pull Request comments to facilitate smooth and fast communication among core members.

## 🏹 Commit Message Convention

Please use the following format for commit messages:
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

### 4. Safety
- Minimize the use of `unsafe` blocks.
- If you use `unsafe`, you **must** add a `// SAFETY:` comment explaining why it is safe.

---

## 🛠 Design Principles (Scalable & Contributor Friendly)

### 1. HAL (Hardware Abstraction Layer)
Strictly separate architecture-dependent code from generic logic to support future architectures (e.g., AArch64).
- **`kernel/`**: Platform-independent logic (scheduler, filesystem, network stack, etc.).
- **`arch/x86_64/`**: CPU-specific implementations (GDT, IDT, page table manipulation, context switching, etc.).
- **Interface**: Kernel core interacts only through abstraction APIs provided by the `arch::` module.

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
