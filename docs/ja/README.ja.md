# ManaOS

**[ManaOS](https://discord.gg/FXTV344M94)** は Rust で開発されたモノリシックな x86_64 UEFI カーネルです。拡張性とコントリビューターへの親しみやすさに重点を置いて設計されており、真の **「開発者のためのOS」** を目指しています。

## 🚀 主な特徴

- **HAL アーキテクチャ**: ハードウェア固有のロジックと、共通のカーネルロジックを厳格に分離。将来の移植性を考慮した設計です。
- **コールバック方式の割り込み配線**: `arch/` は `kernel/` に直接依存せず、登録済みコールバックを通じて割り込みイベントを配送します。
- **Boot/Runtime の分離**: `main.rs` は配線に集中し、boot/runtime モジュールが初期化と tick 処理を担当します。
- **開発者ファーストな API**: 直感的で安全な API を提供（例: 文字描画の `graphics.draw_text` など）。
- **グローバルな協力体制**: 英語を基本としたドキュメント構成に加え、日本語での活発な議論をサポートします。
- **モダンなツールチェーン**: `just` を使用したビルド・実行、`qemu` によるテスト環境の完備。

## 🛠 はじめに

### 必須環境 (Prerequisites)

ManaOS のビルドには以下の環境が必要です。

- **Rust (Nightly channel)**: OS 開発に必要な最新機能（`abi_x86_interrupt` など）を使用するため。
- **QEMU**: 動作確認用のエミュレータ。
- **OVMF.fd**: UEFI ブートに必要なファームウェア。リポジトリのルートディレクトリに配置してください。
- **just**: build、run、lint、smoke test の documented command を実行するため。

### ビルドと実行 (Build and Run)

`just` がインストールされている場合は、以下のコマンドだけでビルドから QEMU での起動まで行われます：

```bash
just
```

`just` を使用しない場合は、OS ごとに用意されたスクリプトを使用できます：

- **Windows**: `run.bat` を実行。
- **Linux/macOS**: `./run.sh` を実行。

## プロジェクトトピック (Project Topics)

ManaOS のドキュメントは、次の topic を軸に読むと把握しやすくなります。

- **Architecture Boundaries**: `main.rs` が composition root、`arch/` が hardware-specific entry、
  `kernel/` が platform-independent policy を所有します。
- **Interrupts And Timers**: interrupt handler は最小限にし、登録済み callback と active
  interrupt-controller backend の acknowledge に閉じます。
- **Memory Ownership**: physical frame、user address space、kernel virtual mapping、DMA buffer、
  guarded stack はそれぞれ明確な owner rule を持ちます。
- **User Processes**: ELF loading、user stack、syscall entry、trap frame、preemption、process metadata、
  future `execve` / `waitpid` を process lifecycle として扱います。
- **Storage And Filesystems**: AHCI、GPT、FAT32、VFS、path normalization、file descriptor、future write
  support を層に分けて扱います。
- **Developer Workflow**: 外部 contributor は PR workflow、maintainer は local verification 後のみ direct
  branch workflow を使います。

## ドキュメントマップ (Documentation Map)

英語文書を正本とし、日本語 companion document を併置しています。

| Topic | English | Japanese |
| --- | --- | --- |
| Project overview | [../../README.md](../../README.md) | [README.ja.md](README.ja.md) |
| Contribution rules | [../../CONTRIBUTING.md](../../CONTRIBUTING.md) | [CONTRIBUTING.ja.md](CONTRIBUTING.ja.md) |
| Agent rules | [../../AGENTS.md](../../AGENTS.md) | 英語正本のみ |
| Maintainer workflow | [../MAINTAINER_WORKFLOW.md](../MAINTAINER_WORKFLOW.md) | [MAINTAINER_WORKFLOW.ja.md](MAINTAINER_WORKFLOW.ja.md) |
| Architecture | [../ARCHITECTURE.md](../ARCHITECTURE.md) | [ARCHITECTURE.ja.md](ARCHITECTURE.ja.md) |
| ACPI and APIC | [../ACPI.md](../ACPI.md) | [ACPI.ja.md](ACPI.ja.md) |
| Address boundaries | [../ADDRESS_BOUNDARIES.md](../ADDRESS_BOUNDARIES.md) | [ADDRESS_BOUNDARIES.ja.md](ADDRESS_BOUNDARIES.ja.md) |
| Memory management | [../MEMORY_MANAGEMENT.md](../MEMORY_MANAGEMENT.md) | [MEMORY_MANAGEMENT.ja.md](MEMORY_MANAGEMENT.ja.md) |
| Kernel stacks | [../KERNEL_STACKS.md](../KERNEL_STACKS.md) | [KERNEL_STACKS.ja.md](KERNEL_STACKS.ja.md) |
| User trap frames | [../USER_TRAP_FRAME.md](../USER_TRAP_FRAME.md) | [USER_TRAP_FRAME.ja.md](USER_TRAP_FRAME.ja.md) |
| User pointer validation | [../USER_POINTER_VALIDATION.md](../USER_POINTER_VALIDATION.md) | [USER_POINTER_VALIDATION.ja.md](USER_POINTER_VALIDATION.ja.md) |
| Filesystem | [../FILESYSTEM.md](../FILESYSTEM.md) | [FILESYSTEM.ja.md](FILESYSTEM.ja.md) |
| Manual QEMU validation | [../MANUAL_QEMU_VALIDATION.md](../MANUAL_QEMU_VALIDATION.md) | [MANUAL_QEMU_VALIDATION.ja.md](MANUAL_QEMU_VALIDATION.ja.md) |
| Task priority | [../TASK_PRIORITY.md](../TASK_PRIORITY.md) | [TASK_PRIORITY.ja.md](TASK_PRIORITY.ja.md) |
| Active TODO | [../../TODO.md](../../TODO.md) | [../../TODO.ja.md](../../TODO.ja.md) |
| Completed TODO archive | [../../TODO_COMPLETED.md](../../TODO_COMPLETED.md) | [../../TODO_COMPLETED.ja.md](../../TODO_COMPLETED.ja.md) |
| Security policy | [../../SECURITY.md](../../SECURITY.md) | [../../SECURITY.ja.md](../../SECURITY.ja.md) |
| Third-party licenses | [../../THIRD_PARTY_LICENSES.md](../../THIRD_PARTY_LICENSES.md) | [../../THIRD_PARTY_LICENSES.ja.md](../../THIRD_PARTY_LICENSES.ja.md) |

## 検証コマンド早見表 (Validation Quick Reference)

変更内容に合う最小の検証から実行し、kernel boundary をまたぐ場合は広い検証へ進みます。

```bash
just fmt
cargo check
cargo check --target x86_64-unknown-uefi
just lint
just storage-smoke
```

- docs-only 変更では `git diff --check` または `git show --check` を使います。
- architecture、kernel/userland、syscall boundary に触る場合は `just lint` を使います。
- boot-visible behavior、storage/filesystem、scheduler、memory ownership、syscall behavior、
  userland runtime behavior に触る場合は `just storage-smoke` を使います。

## 🤝 貢献 (Contributing)

私たちは、日本国内および世界中からのコントリビューターを心より歓迎します！
コーディング規約、安全性の確保、設計の原則、および現在のロードマップについては、詳細なガイドラインである **[CONTRIBUTING.ja.md](CONTRIBUTING.ja.md)** をご覧ください。

アーキテクチャとモジュール責務の詳細は **[ARCHITECTURE.ja.md](ARCHITECTURE.ja.md)** を参照してください。
現在のロードマップと既知のリファクタリング項目は英語正本の **[TODO.md](../../TODO.md)** にまとめています。
日本語でフェーズの意図を確認したい場合は **[TODO.ja.md](../../TODO.ja.md)** を参照してください。

---

## 📄 ライセンス

現在のプロジェクトライセンスは、プロジェクトルートの [LICENSE](../../LICENSE) ファイルを参照してください。

---

開発者コミュニティのために ❤️ を込めて構築されました。
