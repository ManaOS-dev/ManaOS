# メンテナ向け直接ブランチ運用

この運用は、外部コントリビュータが関与していない変更を、メンテナまたは
プロジェクト所有の自動化が扱う場合にだけ使用します。外部コントリビュータは
[`CONTRIBUTING.md`](../../CONTRIBUTING.md) の Pull Request 運用を使用してください。

## 使用する場面

直接ブランチ運用は、ローカルで完全に検証してから `master` に入れられる、
焦点の絞られた変更に使用します。

実験的な作業、レビュー単位が曖昧な大きなリファクタ、外部レビューが必要な変更には
使用しません。`experimental/xxx` ブランチは、検証済みの `feature/xxx`、
`fix/xxx`、`refactor/xxx`、または `docs/xxx` ブランチへ変換するまで
マージしないでください。

## 必須の開始状態

最新でクリーンな `master` から開始します。

```powershell
git switch master
git pull --ff-only origin master
git status --short --branch
git log -1 --oneline
```

`git status` は、`master` が `origin/master` を追跡しており、未コミット変更が
ない状態を示す必要があります。プロジェクトオーナーが意図的に含める変更を残している
場合は、作業ブランチを作る前にその範囲を確認してください。

## ブランチとコミット

1 つの作業単位に絞ったブランチを作成します。

```powershell
git switch -c docs/example-workflow
```

変更内容に合う prefix を使用してください。

- `feature/xxx`: 単一の機能単位。
- `fix/xxx`: バグ修正。
- `refactor/xxx`: 明示しない限り挙動変更を含まないコード整理。
- `docs/xxx`: ドキュメントのみの変更。

コミットメッセージは英語で、簡潔な命令形の要約にします。

```powershell
git add <changed-files>
git commit -m "Document direct maintainer workflow"
```

## ローカル検証

まず対象に合う最小の検証を実行し、kernel、userland、architecture、runtime の
境界を跨ぐ場合はより広い検証を実行します。

ドキュメントのみの変更:

```powershell
git diff --check
```

Rust コード変更:

```powershell
just fmt
cargo check
cargo check --target x86_64-unknown-uefi
cargo clippy --all-targets --all-features -- -D warnings
```

kernel/userland 境界、architecture 配線、syscall、memory ownership、
interrupt routing、storage、filesystem、scheduler、または boot-visible な
runtime 挙動に触れる変更:

```powershell
just lint
just storage-smoke
```

実行しなかった検証がある場合は、マージ前にその理由を記録してください。

## マージ、プッシュ、後片付け

作業ブランチを検証した後だけマージします。

```powershell
git switch master
git merge --ff-only docs/example-workflow
git push origin master
git branch -d docs/example-workflow
```

作業ブランチを `origin` に push していた場合は、`master` の push 後に削除します。

```powershell
git push origin --delete docs/example-workflow
```

`--ff-only` が失敗した場合は、強制的に直さず、まずブランチ履歴を確認してください。
プロジェクトオーナーが明示的に依頼しない限り、履歴修復のために破壊的なコマンドを
使用してはいけません。

後片付け後、最終状態を確認します。

```powershell
git status --short --branch
git log -1 --oneline
```

`master` はクリーンで、push 済みで、検証済みコミットを指している必要があります。
