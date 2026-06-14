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

## 前提条件

- README にある build tool が入っていること。
- repository root に `OVMF.fd` が存在すること。
- `just run`、`run.bat`、または `./run.sh` で QEMU boot できること。
- storage validation では、fixture layout を変えた場合に GPT disk image を再生成すること。

```powershell
just storage-smoke
```

可能な場合は自動 smoke を先に使います。manual validation は、interactive console の観察や、
serial assertion より目視しやすい workflow の確認に使います。

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

## 各 step が証明すること

- boot milestone: UEFI handoff、paging、heap、serial logging、core kernel initialization が
  runtime loop まで到達しています。
- filesystem smoke log: storage probing、GPT selection、FAT32 mount、`/dev` registration、
  directory descriptor iteration が動いています。
- `cat /disk/hello.txt`: AHCI read、FAT32 file lookup、VFS file descriptor read、console command
  dispatch、text rendering が接続されています。
- pipeline command: command output buffering、pipe dispatch、downstream command input handling が
  single-pipe console path で動いています。

## 失敗時に見る場所

- boot 前に止まる場合: QEMU / OVMF / ESP setup。
- `ManaOS Kernel is alive.` 前に止まる場合: boot initialization、paging、serial logging。
- storage smoke log が欠ける場合: AHCI、GPT、FAT32、VFS mount。
- `cat` が失敗する場合: path normalization、file lookup、descriptor read。
- pipeline が失敗する場合: console command dispatch、pipeline buffer、grep command。

manual validation failure を報告する場合は、実行した console command と該当 serial line を正確に
残してください。
