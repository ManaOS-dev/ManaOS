# Manual QEMU Validation

Use this checklist after storage, filesystem, syscall, or console changes.

1. Boot the kernel with the normal QEMU command for this repository.
2. Confirm the serial log reaches `ManaOS Kernel is alive.`.
3. Confirm the filesystem smoke logs include `/dev` directory listing and directory handle checks.
4. In the kernel console, run `cat /disk/hello.txt`.
5. Confirm the console prints `hello from FAT32`.

The `cat /disk/hello.txt` check verifies the storage path from AHCI through GPT,
FAT32, the virtual filesystem, and the kernel console command dispatcher.
