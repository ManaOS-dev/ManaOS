# セキュリティポリシー

## サポート対象バージョン

ManaOS は現在 pre-release software です。セキュリティ修正は、active development
branch 上で扱います。安定版ブランチや長期サポート版はまだ定義していません。

## 脆弱性の報告

疑わしい脆弱性を見つけた場合は、public issue を作成する前に、maintainer へ非公開で
報告してください。

報告には、可能な範囲で以下を含めてください。

- 影響を受ける commit または branch。
- 再現手順。
- 期待される影響範囲。
- QEMU log、serial log、panic output。
- 変更した local configuration があればその内容。

攻撃コードは、再現に必要な最小限を超えて含めないでください。再現用の入力、ログ、
期待される挙動と実際の挙動を優先してください。

## ManaOS 固有の注意点

ManaOS は OS kernel であり、通常の application bug よりも影響範囲が広くなります。
特に以下は security-sensitive として扱います。

- user pointer validation bypass。
- kernel/user page permission の誤り。
- writable executable mapping。
- syscall argument validation の欠落。
- interrupt、exception、syscall path での不正な lock や allocation。
- frame allocator の double free、owner mismatch、use-after-free。
- storage parser や ELF parser の境界チェック不足。

報告時点で exploitability が不明でも、kernel memory corruption、権限境界の破壊、
任意の physical/virtual address access につながる可能性がある場合は、非公開報告を
優先してください。
