# ManaOS 貢献ガイドライン (CONTRIBUTING)

ManaOS プロジェクトへの参加を歓迎します！ManaOS は「開発者のための OS」であり、皆様のあらゆる貢献を大切にしています。このドキュメントでは、プロジェクトへの参加方法、コーディング規約、および設計原則について詳しく説明します。

## 🤝 コントリビューションのワークフロー

外部コントリビュータはプルリクエストのワークフローを使用してください。
対象変更に外部コントリビュータが関与していない場合、メンテナおよびプロジェクト所有の自動化は、ローカル検証後に `AGENTS.md`
で必須化され、[`MAINTAINER_WORKFLOW.ja.md`](MAINTAINER_WORKFLOW.ja.md) に記載された直接ブランチ運用を使用できます。

1. **フォーク (Fork)**: リポジトリを自分のアカウントにフォークします。
2. **ブランチ作成**: 機能追加や修正のためのブランチを作成します: `git checkout -b feature/your-awesome-feature` または `git checkout -b fix/your-bug`
3. **コミット**: 明確で簡潔なメッセージとともに変更をコミットします。
4. **整形と静的解析**: 後述するツールを使用して、コードの品質を確認します。
5. **プッシュと PR**: 自分のフォークにプッシュし、`master` に対してプルリクエストを作成します。

## 🌿 ブランチポリシー

| ブランチ | 用途 |
|---|---|
| `master` | 検証済みの変更だけが入る、いつでもビルド・起動可能な状態 |
| `feature/xxx` | 単一の機能単位 |
| `fix/xxx` | バグ修正 |
| `refactor/xxx` | 明示しない限り挙動変更を含まないコード整理 |
| `docs/xxx` | ドキュメントのみの変更 |
| `experimental/xxx` | 実験的な作業。検証済みブランチに変換するまでマージしない |

- `feature/xxx`、`fix/xxx`、`refactor/xxx`、`docs/xxx` からの PR は
  `master` ブランチをターゲットにします。
- 各ブランチはレビュー可能な 1 つの作業単位に絞ってください。
- マージ後は作業ブランチを削除してください。

---

## 🇯🇵 日本語でのコミュニケーションについて
GitHub の Issues や Pull Request のコメントでは、**日本語での議論を歓迎します**。コアメンバー間での迅速かつ深い意思疎通を優先するためです。ただし、コード、コメント、コミットメッセージについては、以下の「言語ポリシー」に従ってください。

---

## 📝 言語ポリシー

- **コードとインラインコメント**: **英語のみ**。グローバルな協力体制の維持と、将来的なツール統合（ドキュメント生成など）を円滑にするためです。
- **コミットメッセージ**: **英語**。簡潔な命令形の要約を書いてください。
  Conventional Commit の prefix は有用な場合のみ任意で使用します。
- **ドキュメント (Markdown)**: 英語を正本としますが、`docs/ja/` 配下に同等の内容を持つ日本語ドキュメントを維持します。

---

## 🏹 コミットメッセージの規約

明確な英語のコミットメッセージを使用してください。Conventional Commit の
prefix は使用可能ですが必須ではありません：
- `feat`: 新機能の追加
- `fix`: バグの修正
- `docs`: ドキュメントのみの変更
- `style`: コードの意味に影響を与えない変更（ホワイトスペース、フォーマットなど）
- `refactor`: バグ修正や機能追加を行わないコードの整理
- `perf`: パフォーマンス向上
- `test`: テストの追加や修正
- `chore`: ビルドプロセスやライブラリの更新など

---

## 🛠 コーディング標準

高いコード品質を維持するため、以下のルールを厳守してください。

### 1. コードのフォーマット
すべての Rust コードは `rustfmt` で整形されている必要があります。コミット前に以下のコマンドを実行してください：
```bash
just fmt
```

### 2. 静的解析 (Lint)
`clippy` を使用して一般的なミスの検出とベストプラクティスの適用を行います。PR は警告（warnings）がない状態でパスする必要があります：
```bash
just lint
```

### 3. ドキュメントの記述
- すべてのパブリックな項目 (`pub` 関数、構造体、列挙型など) には、`///` を使用したドキュメントコメントを記述してください。
- 内部的なロジックが複雑な場合は、適宜インラインコメントを追加してください。

### 4. 命名
- `fb_info`、`h`、`v` のような不明瞭なローカル略語は避けてください。
- `PCI`、`AHCI`、`GPT`、`FAT32`、`UEFI`、`GDT`、`IDT`、`GOP`、`PIC`、
  `PIT`、`APIC`、`IOAPIC`、`LBA`、`FIS`、`DMA`、`PRDT` など、読みやすさを
  上げるドメイン標準の頭字語は使用できます。
- ログカテゴリや診断メッセージでは簡潔な頭字語を優先します。

### 5. モジュール境界
- `mod.rs` は薄く保ち、責務コメント、モジュール宣言、re-export、小さな
  public API の転送だけにしてください。
- 処理ロジックは `queue`、`decoder`、`state`、`hardware` など、責務ごとの
  sibling module に移動してください。

### 6. 安全性 (Safety)
- `unsafe` ブロックの使用は最小限に留めてください。
- `unsafe` を使用する場合は、**必ず** `// SAFETY:` コメントを添え、なぜその操作が安全であるかを論理的に説明してください。

## 📚 ドキュメント標準

ドキュメント変更は、実装の後付け説明ではなく、engineering contract の一部として扱います。

- 英語文書を正本とします。
- 日本語 companion document が存在する場合は、英語版と同じ運用上の意味を説明します。
- generated file は手で編集しません。`THIRD_PARTY_LICENSES.md` は `just licenses` で再生成します。
- `TODO.md` には未完了作業だけを残します。完了済み項目は、実装 branch の検証後に
  `TODO_COMPLETED.md` へ移動します。
- architecture、memory、syscall、storage、scheduler、userland behavior を変更する場合は、
  同じ branch で近い design document も更新します。
- `docs/` 配下に新しい Markdown を追加する場合は、日本語 companion を追加するか、
  追加しない理由を明確にし、contributor-facing document なら `README.md` の documentation map も更新します。
- あいまいな roadmap text より、具体的な invariant、ownership rule、failure mode、
  validation command を優先します。

## ✅ 検証マトリクス

変更内容に合う最小の検証から始め、runtime boundary をまたぐ場合は広い検証へ進みます。

| Change type | Minimum verification |
| --- | --- |
| docs-only | `git diff --check` または `git show --check` |
| formatting-only Rust changes | `just fmt` |
| kernel Rust behavior | `cargo check --target x86_64-unknown-uefi` |
| userland no-std behavior | `cargo clippy --manifest-path userland/Cargo.toml --target x86_64-unknown-none --target-dir target/userland --lib --bin file_demo --bin bad_pointer_demo --bin smoke_demo -- -D warnings` |
| architecture または kernel/userland boundary | `just lint` |
| boot-visible runtime behavior | `just storage-smoke` |

local で command を実行できない場合は、正確な理由と必要な follow-up validation を記録します。

---

## 🛠 設計原則 (拡張性とコントリビュートのしやすさ)

### 1. HAL (Hardware Abstraction Layer)
アーキテクチャ依存のコードと、OS のコアロジックを厳格に分離します。これにより、将来的な他アーキテクチャ（AArch64 など）への対応を容易にします。
- **`src/kernel/`**: プラットフォームに依存しないロジック（スケジューラ、ファイルシステム、ネットワークスタックなど）。
- **`src/arch/x86_64/`**: CPU 固有の実装（GDT, IDT, ページテーブル操作、コンテキストスイッチなど）。
- **インターフェース**: カーネルコアは `arch::` モジュールが提供する抽象化 API を通じてのみハードウェアを操作します。
- **割り込み境界**: `arch/` は `kernel::...` を直接呼びません。割り込みハンドラは `main.rs` が登録したコールバックへ配送します。

### 2. トレイト駆動のドライバ設計
デバイスドライバを Rust のトレイトで抽象化し、モジュール式の拡張を可能にします。
- **Console トレイト**: シリアルポートや GOP フレームバッファなどを、共通の書き込み操作として扱います。
- **BlockDevice トレイト**: AHCI, NVMe などの異なるディスクアクセスを抽象化します。

### 3. 型安全なメモリ管理 (Newtype パターン)
物理アドレスと仮想アドレスを型レベルで厳密に区別し、誤用によるバグを防ぎます。
- `PhysAddr(u64)` と `VirtAddr(u64)` による明示的な分離。
- すべての `unsafe` 操作は最小限のモジュールに隔離し、型安全なラッパーを提供します。

### 4. 開発者体験 (DX) の重視
- **標準化されたツール**: `just` を使用して、ビルド、実行、テストをワンコマンドで完結させます。
- **一貫性**: `rustfmt` と `clippy` を CI で強制し、誰が書いても美しいコードベースを維持します。

---

## 📅 ロードマップと TODO

ManaOS の現在の開発ステータスと、今後のタスクについては **[TODO.md](../../TODO.md)** を参照してください。
モジュール責務と割り込み配線の詳細は **[ARCHITECTURE.ja.md](ARCHITECTURE.ja.md)** を参照してください。

---

このドキュメントは、英語版の **[CONTRIBUTING.md](../../CONTRIBUTING.md)** と同等の情報を提供しています。
