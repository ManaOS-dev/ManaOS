# Manual QEMU Validation

この文書は [`../MANUAL_QEMU_VALIDATION.md`](../MANUAL_QEMU_VALIDATION.md) の日本語版です。
storage、filesystem、syscall、console、user shell に関わる変更後に、手動 QEMU で確認する
最小 checklist を示します。可能な限り自動 smoke を先に使い、manual validation は graphical console、
QEMU window の挙動、serial assertion より目視しやすい workflow の確認に使います。

## 前提条件

- README にある build tool が入っていること。
- repository root に `OVMF.fd` が存在すること。
- `just run`、`run.bat`、または `./run.sh` で QEMU boot できること。
- `/disk` と userland を確認する場合は、manual run の前に GPT disk image を更新すること。

```powershell
just storage-smoke
```

`just run` は kernel を build して QEMU を起動しますが、`disk.img` が存在しない場合にだけ空の image を
作ります。userland binary や disk layout を変更した後に FAT32 fixture を作り直す command ではありません。
`/disk/bin/smoke_demo`、`/disk/bin/file_demo`、`/disk/bin/user_shell` を確認するときは
`just storage-smoke` を先に使ってください。

## 起動コマンド

通常は次の command を使います。

```powershell
just run
```

Windows では `just run` が `run.bat` に委譲します。Linux と macOS では `./run.sh` に委譲します。
どちらも GTK display 付きの QEMU を起動し、serial output は terminal に出します。

## 手順

1. storage fixture、userland、syscall、scheduler、process lifecycle を変更した後は
   `just storage-smoke` を実行します。
2. 自動 smoke が `[storage-smoke] PASS` を出すことを確認します。
3. `just run` で graphical QEMU を起動します。
4. serial output が `ManaOS Kernel is alive.` に到達することを確認します。
5. storage milestone に `Registered FAT32 file backend for virtual filesystem: path=/disk/bin/user_shell`
   が含まれることを確認します。
6. experimental user shell の serial milestone として、`Initial user shell smoke started`、
   `user shell ready`、`Initial user shell keyboard stdin wait verified`、
   `Initial user shell keyboard stdin prepared: bytes=5`、`User task read completed`、
   `Initial user shell smoke passed` が出ることを確認します。
7. `Initial user shell smoke passed` の後、graphical console で `pwd` を実行し、
   kernel console が使えることを確認します。
8. `cat /disk/hello.txt` を実行します。
9. console に `hello from FAT32` が表示されることを確認します。
10. `cat /disk/hello.txt | grep FAT32` を実行します。
11. console に `hello from FAT32` が表示され、serial log に `Pipeline command completed` が
    出ることを確認します。
12. QEMU を終了するには GTK window を閉じます。terminal monitor session を使っている場合は
    `quit` でも emulator を終了できます。

## Experimental User Shell の現在の範囲

現在の user shell path は boot-smoke owned です。kernel は process lifecycle smoke gate の後に
`/disk/bin/user_shell` を自動起動し、standard input を `/dev/keyboard` へ接続し、keyboard queue が
空のときに EOF ではなく block することを確認し、`exit\n` を注入して clean exit を検証します。

これはまだ永続的な手動 shell session ではありません。boot 後に user shell prompt が残ることや、
kernel console command から手動起動できることは期待しないでください。現時点の manual QEMU validation は、
上記 serial milestone を観察し、その後に control が kernel console へ戻っていることを確認します。

## 各 step が証明すること

- boot milestone: UEFI handoff、paging、heap、serial logging、core kernel initialization が
  runtime loop まで到達しています。
- storage milestone: AHCI probing、GPT selection、FAT32 mount、userland ELF registration、
  `/dev` registration、directory descriptor iteration が動いています。
- user shell milestone: `spawn`、process-owned descriptor、keyboard-backed stdin wait/wake、
  no-std shell command handling、`waitpid`、child collection、clean shell exit が接続されています。
- `cat /disk/hello.txt`: AHCI read、FAT32 file lookup、VFS file descriptor read、
  console command dispatch、text rendering が接続されています。
- pipeline command: command output buffering、pipe dispatch、downstream command input handling が
  single-pipe console path で動いています。

## 失敗時に見る場所

- QEMU が起動しない場合: QEMU installation と `OVMF.fd` location。
- `ManaOS Kernel is alive.` 前に止まる場合: early boot、paging、heap、serial initialization。
- `/disk/bin/user_shell` が serial log に出ない場合: `just storage-smoke` で GPT disk fixture を
  作り直し、`scripts/create_gpt_disk_image.ps1` の失敗を確認します。
- shell は起動するが keyboard stdin wait/wake milestone が出ない場合: keyboard stdin queueing、
  `read` blocking、scheduler wakeup、`scripts/run_storage_smoke.ps1` の expected pattern。
- shell smoke 後に kernel console が使えない場合: rendering より先に shell exit collection と
  console smoke log を確認します。
- `cat` が file を開けるが byte が想定と違う場合: FAT32 cluster traversal と storage sector read。
- pipeline が失敗し `cat` は動く場合: console command output buffer、pipe parsing、`grep` command input。

manual validation failure を報告する場合は、実行した console command と該当 serial line を正確に
残してください。
