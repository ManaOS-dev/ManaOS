# ManaOS

**[ManaOS](https://discord.gg/FXTV344M94)** is a monolithic x86_64 UEFI kernel developed in Rust, designed with a focus on scalability and contributor friendliness—truly an **"OS for Developers."**

## 🚀 Key Features

- **HAL Architecture**: Strict separation between hardware-specific and generic kernel logic.
- **Developer-First API**: Ergonomic and intuitive APIs (e.g., `graphics.DrawText`).
- **Global Collaboration**: English-first documentation and standard PR-based workflow.
- **Modern Tooling**: Seamless build and run experience with `just` and `qemu`.

## 🛠 Getting Started

### Prerequisites

- [Rust (Nightly)](https://rustup.rs/)
- [QEMU](https://www.qemu.org/)
- `OVMF.fd` (Place it in the root directory)

### Build and Run

If you have `just` installed:

```bash
just
```

Alternatively, use the provided scripts:

- **Windows**: `run.bat`
- **Linux/macOS**: `./run.sh`

## 🤝 Contributing

We welcome contributors from all over the world! Please check our **[CONTRIBUTING.md](CONTRIBUTING.md)** for guidelines on coding standards, language policy, design principles, and our roadmap.

---

## 🇯🇵 日本語ドキュメント (Japanese Documentation)

日本語のドキュメントは **[docs/ja/README.ja.md](docs/ja/README.ja.md)** に用意されています。

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details (or choice of your license).

---

Built with ❤️ for the developer community.
