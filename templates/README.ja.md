# ManaOS Templates 日本語ガイド

このディレクトリの template は、contributor と agent がコピーして使うための出発点です。
generated output ではありません。コピー後は、commit 前にすべての placeholder を実際の内容へ置き換えてください。

## Template Catalog

| Template | Use |
| --- | --- |
| `driver.rs.template` | 新しい driver module facade または小さな driver API の出発点。 |
| `module_mod.rs.template` | 新しい `mod.rs` の ownership block と薄い module facade。 |
| `documentation.md.template` | 英語の design / validation document。 |
| `documentation.ja.md.template` | contributor-facing document の日本語 companion。 |
| `commit-message.template` | non-trivial change 用 commit message skeleton。 |

## 使用ルール

- template を使う前に `AGENTS.md` と `CONTRIBUTING.md` を読んでください。
- Rust のコメントと Rust doc comment は英語で書きます。
- `mod.rs` は ownership docs、module declaration、re-export、小さな forwarding API だけにします。
- main-loop processor には `process_*`、interrupt 側 event ingestion には `push_*` を使います。
- すべての static は private にし、state は function 経由で公開します。
- commit 前に、コピー先ファイルへ placeholder が残っていないか確認します。

```powershell
rg -n "Replace with|<[^>]+>" src docs
```

## 検証

template-only または Markdown-only 変更では以下を実行します。

```powershell
git diff --check
```

template をコピーして Rust code として使った場合は、変更した subsystem に応じて
`CONTRIBUTING.md` の検証コマンドを実行してください。
