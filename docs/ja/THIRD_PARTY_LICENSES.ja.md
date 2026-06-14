# Third-Party Licenses 日本語ガイド

第三者ライセンス一覧の正本は [`THIRD_PARTY_LICENSES.md`](../../THIRD_PARTY_LICENSES.md) です。
この日本語ファイルは、一覧の読み方と更新手順を説明します。

## このファイルの位置づけ

`THIRD_PARTY_LICENSES.md` は `cargo-license` と
`scripts/generate_third_party_licenses.ps1` によって生成されるメタデータです。crate 名、
version、license、repository URL は英語版を正本として扱ってください。

日本語版では、生成結果そのものを翻訳して複製しません。依存関係の version や license は
変更されやすく、手作業で翻訳した表を持つと正本とずれるためです。

## 更新方法

依存関係を変更した後は、以下を実行して英語版の license inventory を再生成します。

```powershell
just licenses
```

生成結果に差分が出た場合は、crate の license、repository、binary asset の扱いを確認し、
必要に応じて commit に含めます。

## Binary Assets

リポジトリには `esp/Inter.ttf` と `esp/NotoSansJP.ttf` が含まれます。再配布前には、
正確な upstream source と font license text を確認してください。

## 注意

この日本語ファイルは説明用です。法的判断や配布判断では、必ず英語版の生成済み一覧と
各 upstream license text を確認してください。
