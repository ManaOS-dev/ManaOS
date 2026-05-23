#!/bin/bash
set -e

echo "[build] Compiling kernel..."
cargo build --target x86_64-unknown-uefi

echo "[build] Setting up ESP..."
mkdir -p esp/EFI/BOOT
cp target/x86_64-unknown-uefi/debug/mana_os.efi esp/EFI/BOOT/BOOTX64.EFI

echo "[run] Starting QEMU..."
qemu-system-x86_64 \
  -display gtk,zoom-to-fit=on \
  -drive if=pflash,format=raw,readonly=on,file=OVMF.fd \
  -drive format=raw,file=fat:rw:esp \
  -serial stdio
