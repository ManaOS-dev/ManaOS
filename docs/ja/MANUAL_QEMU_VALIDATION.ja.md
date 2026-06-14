# Manual QEMU Validation

この文書は [`../MANUAL_QEMU_VALIDATION.md`](../MANUAL_QEMU_VALIDATION.md) の日本語版です。
storage、filesystem、syscall、console に関わる変更後に、手動 QEMU で確認する最小 checklist を
示します。

## 使う場面

以下の変更を行った場合は、`just storage-smoke` だけでなく、必要に応じて手動 QEMU でも確認します。

- storage driver。
- GPT / FAT32 parser。
- VFS と file descriptor。
- syscall surface。
- kernel console command。
- userland smoke program。

## 手順

1. この repository の通常 QEMU command で kernel を boot します。
2. serial log が `ManaOS Kernel is alive.` に到達することを確認します。
3. filesystem smoke log に `/dev` directory listing と directory handle check が含まれることを確認します。
4. kernel console で `cat /disk/hello.txt` を実行します。
5. console に `hello from FAT32` が表示されることを確認します。
6. serial log に `Pipeline command smoke passed` が出ることを確認します。
7. kernel console で `cat /disk/hello.txt | grep FAT32` を実行します。
8. console に `hello from FAT32` が表示され、serial log に `Pipeline command completed` が出ることを確認します。

## 何を検証しているか

`cat /disk/hello.txt` は、以下の経路をまとめて検証します。

- AHCI block device。
- GPT partition parsing。
- FAT32 filesystem backend。
- virtual filesystem mount と path traversal。
- file descriptor read。
- kernel console command dispatcher。
- console output rendering。

pipeline check は、console command output を次の command に渡す最小 pipeline 経路も確認します。

## 失敗時に見る場所

- boot 前に止まる場合: QEMU / OVMF / ESP setup。
- `ManaOS Kernel is alive.` 前に止まる場合: boot initialization、paging、serial logging。
- storage smoke log が欠ける場合: AHCI、GPT、FAT32、VFS mount。
- `cat` が失敗する場合: path normalization、file lookup、descriptor read。
- pipeline が失敗する場合: console command dispatch、pipeline buffer、grep command。
