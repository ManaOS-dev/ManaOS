# ManaOS Agent Coding Rules 日本語ガイド

このファイルは [`AGENTS.md`](AGENTS.md) の日本語ガイドです。AI assistant が従う
必須ルールの正本は英語版 `AGENTS.md` です。迷った場合は英語版を優先してください。

## 最初に必ず確認すること

作業を始める前に、PowerShell では次を実行して `src\` の構成を確認します。

```powershell
Get-ChildItem -Path src\ -File -Recurse | Resolve-Path -Relative
```

あわせて Git の状態も確認します。

```powershell
git status --short --branch
git log -1 --oneline
```

## 命名ルール

短すぎるローカル略語は避けます。`fb_info`、`h`、`v` のような名前ではなく、
`framebuffer_info`、`width`、`height` のように意味が分かる名前を使います。

ただし、`PCI`、`AHCI`、`GPT`、`FAT32`、`UEFI`、`GDT`、`IDT`、`GOP`、`PIC`、
`PIT`、`APIC`、`IOAPIC`、`LBA`、`FIS`、`DMA`、`PRDT` などの domain-standard acronym
は使用できます。診断ログでは、これらの acronym を使ったほうが読みやすい場合があります。

関数名は役割で揃えます。

- interrupt handler から呼ばれる push 系 API は `push_*`。
- main loop 側の処理は `process_*`。
- 状態読み取りは `get_*`。
- 初期化は `init` または `initialize`。

## module 境界

各 module は1つの責務だけを持ちます。`mod.rs` の先頭には、その module が所有するもの、
所有しないもの、public API を `//!` doc comment で書きます。

`mod.rs` は薄く保ちます。module 宣言、必要な re-export、小さな forwarding API だけにし、
処理本体は `state.rs`、`queue.rs`、`decoder.rs`、`hardware.rs` などの sibling module
へ置きます。

特に重要な依存ルール:

- `arch/` は `kernel/` に依存しません。
- `kernel/driver/display/` は `kernel/driver/input/` に依存しません。
- `kernel/driver/` は `kernel/memory/` と `kernel/sync/` に依存できます。
- `main.rs` だけが全体を配線する composition root です。

## interrupt wiring

`arch/` の interrupt handler は最小限の仕事だけを行います。

- hardware state を読む。
- interrupt controller に EOI を送る。
- `main.rs` が登録した callback を呼ぶ。

`arch/` から `kernel::...` を直接呼んではいけません。timer、keyboard、mouse などの
processor は `main.rs` が登録し、kernel 側の event routing は `kernel::interrupt` が
所有します。

## static と unsafe

static は private にし、状態は function 経由で公開します。`pub static` は使いません。

`Mutex<bool>` や `Mutex<u64>` ではなく、可能な場合は `AtomicBool` や `AtomicU64` を
使います。

`unsafe` block は最小限にし、必ず近くに `// SAFETY:` コメントを書きます。コメントは
「なぜ安全か」を具体的に説明する必要があります。

## documentation

すべての `pub` function、struct、enum には Rust の `///` doc comment が必要です。
JSDoc style は使いません。`#![deny(missing_docs)]` が有効なので、missing docs は
compile error になります。

## Git workflow

agent は task branch で作業します。検証後に `master` へ merge し、`origin/master` へ
push し、作業 branch を削除します。

基本手順:

```powershell
git switch master
git pull --ff-only origin master
git switch -c docs/example-task
# edit and verify
git commit -m "Use an English imperative summary"
git switch master
git merge --ff-only docs/example-task
git push origin master
git branch -d docs/example-task
```

Rust code 変更では `cargo fmt`、`cargo check`、
`cargo clippy --all-targets --all-features` を実行します。kernel/userland boundary に
触る場合は `just lint` も実行します。boot-visible behavior に触る場合は
`just storage-smoke` を優先して検証します。
