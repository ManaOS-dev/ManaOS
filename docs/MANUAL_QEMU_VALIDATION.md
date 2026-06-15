# Manual QEMU Validation

Use this checklist after storage, filesystem, syscall, console, or user shell
changes. Prefer the automated smoke path first. Manual validation is for
observing the graphical console, QEMU window behavior, and workflows that are
easier to see than to assert from serial logs.

## Prerequisites

- Build tools from the README are installed.
- `OVMF.fd` exists in the repository root.
- QEMU can boot the project through `just run`, `run.bat`, or `./run.sh`.
- For `/disk` and userland validation, refresh the GPT disk image before the
  manual run:

```powershell
just storage-smoke
```

`just run` builds the kernel and starts QEMU, but it only creates `disk.img` if
the file is missing. It does not rebuild the FAT32 fixture after userland binary
or disk-layout changes. Use `just storage-smoke` when validating
`/disk/bin/smoke_demo`, `/disk/bin/file_demo`, or `/disk/bin/user_shell`.

## Boot Commands

Use the normal project run command for your platform:

```powershell
just run
```

On Windows, `just run` delegates to `run.bat`. On Linux and macOS, it delegates
to `./run.sh`. Both paths boot QEMU with a GTK display and serial output in the
terminal.

## Checklist

1. Run `just storage-smoke` after storage fixture, userland, syscall, scheduler,
   or process lifecycle changes.
2. Confirm the automated smoke reports `[storage-smoke] PASS`.
3. Boot the graphical QEMU run with `just run`.
4. Confirm the serial output reaches `ManaOS Kernel is alive.`.
5. Confirm the storage milestones include `Registered FAT32 file backend for
   virtual filesystem: path=/disk/bin/user_shell`.
6. Confirm the experimental user shell milestones appear in serial output:
   `Initial user shell smoke started`, `user shell ready`,
   `Initial user shell keyboard stdin wait verified`,
   `Initial user shell keyboard stdin prepared: bytes=5`,
   `User task read completed`, and `Initial user shell smoke passed`.
7. After `Initial user shell smoke passed`, confirm the kernel console remains
   available by running `pwd` in the graphical console.
8. Run `cat /disk/hello.txt`.
9. Confirm the console prints `hello from FAT32`.
10. Run `cat /disk/hello.txt | grep FAT32`.
11. Confirm the console prints `hello from FAT32` and the serial log reports
    `Pipeline command completed`.
12. Leave QEMU by closing the GTK window. If you are using a terminal monitor
    session, `quit` also terminates the emulator.

## Experimental User Shell Scope

The current user shell path is boot-smoke owned. The kernel starts
`/disk/bin/user_shell` automatically after the process lifecycle smoke gate,
connects standard input to `/dev/keyboard`, proves that an empty keyboard queue
blocks the shell instead of returning EOF, injects `exit\n`, and then verifies
that the shell exits cleanly.

This is not a persistent manual shell session yet. Do not expect a user shell
prompt to remain available after boot, and do not expect a kernel console
command to launch it manually. Manual QEMU validation currently observes the
serial milestones above and then verifies that control has returned to the
kernel console.

## What Each Step Proves

- Boot milestone: UEFI handoff, paging, heap, serial logging, and core kernel
  initialization reached the runtime loop.
- Storage milestones: AHCI probing, GPT selection, FAT32 mount, userland ELF
  registration, `/dev` registration, and directory descriptor iteration are
  alive.
- User shell milestones: `spawn`, process-owned descriptors, keyboard-backed
  stdin wait/wake, no-std shell command handling, `waitpid`, child collection,
  and clean shell exit are connected.
- `cat /disk/hello.txt`: AHCI read, FAT32 file lookup, VFS file descriptor
  read, console command dispatch, and text rendering work together.
- Pipeline command: command output buffering, pipe dispatch, and downstream
  command input handling work for the single-pipe console path.

## Troubleshooting

- If QEMU never opens, check QEMU installation and `OVMF.fd` location.
- If boot stops before `ManaOS Kernel is alive.`, inspect early boot, paging,
  heap, and serial initialization changes.
- If `/disk/bin/user_shell` is missing from the serial log, run
  `just storage-smoke` to rebuild the GPT disk fixture and inspect
  `scripts/create_gpt_disk_image.ps1` failures.
- If the shell starts but does not report keyboard stdin wait/wake milestones,
  inspect keyboard stdin queueing, `read` blocking, scheduler wakeups, and
  `scripts/run_storage_smoke.ps1` expected patterns.
- If the kernel console is unavailable after the shell smoke, inspect shell exit
  collection and console smoke logs before debugging rendering.
- If `cat` opens the file but prints unexpected bytes, inspect FAT32 cluster
  traversal and storage sector reads.
- If the pipeline fails but `cat` works, inspect console command output buffers,
  pipe parsing, and `grep` command input handling.

Record the exact serial lines and console commands when reporting a manual
validation failure.
