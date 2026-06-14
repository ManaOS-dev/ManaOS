# GEMINI.md 日本語版

このリポジトリの coding rule は [`AGENTS.md`](AGENTS.md) が正本です。作業する
assistant は、`AGENTS.md` を厳密に読み、そのルールに従ってください。

特に重要な点:

- 最初に `src\` のファイル構成を確認します。
- `arch/` は `kernel/` に直接依存しません。
- `main.rs` が composition root です。
- `mod.rs` は薄く保ち、責務コメントを置きます。
- public item には Rust `///` doc comment が必要です。
- unsafe block には必ず `// SAFETY:` コメントを置きます。
- code、comment、commit message は英語です。
- 会話や issue/PR discussion では日本語を使用できます。
