# Manual QEMU Validation

Use this checklist after storage, filesystem, syscall, or console changes.

## Prerequisites

- Build tools from the README are installed.
- `OVMF.fd` exists in the repository root.
- QEMU can boot the project through `just run`, `run.bat`, or `./run.sh`.
- For storage validation, regenerate the GPT disk image when the fixture layout
  changes:

```powershell
just storage-smoke
```

Use the automated smoke first when possible. Manual validation is for observing
the interactive console and confirming workflows that are easier to see than to
assert from serial logs.

## Checklist

1. Boot the kernel with the normal QEMU command for this repository.
2. Confirm the serial log reaches `ManaOS Kernel is alive.`.
3. Confirm the filesystem smoke logs include `/dev` directory listing and directory handle checks.
4. In the kernel console, run `cat /disk/hello.txt`.
5. Confirm the console prints `hello from FAT32`.
6. Confirm the serial log reports `Pipeline command smoke passed`.
7. In the kernel console, run `cat /disk/hello.txt | grep FAT32`.
8. Confirm the console prints `hello from FAT32` and the serial log reports
   `Pipeline command completed`.

The `cat /disk/hello.txt` check verifies the storage path from AHCI through GPT,
FAT32, the virtual filesystem, and the kernel console command dispatcher.

## What Each Step Proves

- Boot milestone: UEFI handoff, paging, heap, serial logging, and core kernel
  initialization reached the runtime loop.
- Filesystem smoke logs: storage probing, GPT selection, FAT32 mount, `/dev`
  registration, and directory descriptor iteration are alive.
- `cat /disk/hello.txt`: AHCI read, FAT32 file lookup, VFS file descriptor read,
  console command dispatch, and text rendering work together.
- Pipeline command: command output buffering, pipe dispatch, and downstream
  command input handling work for the single-pipe console path.

## Troubleshooting

- If QEMU never opens, check QEMU installation and `OVMF.fd` location.
- If boot stops before the kernel alive line, inspect early boot, paging, heap,
  and serial initialization changes.
- If `/disk` is missing, inspect AHCI probe, GPT parsing, FAT32 mount, and disk
  fixture generation.
- If `cat` opens the file but prints unexpected bytes, inspect FAT32 cluster
  traversal and storage sector reads.
- If the pipeline fails but `cat` works, inspect console command output buffers,
  pipe parsing, and `grep` command input handling.

Record the exact serial lines and console commands when reporting a manual
validation failure.
