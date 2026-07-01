#!/usr/bin/env bash
set -euo pipefail

KERNEL_TARGET="x86_64-unknown-uefi"
USERLAND_TARGET="x86_64-unknown-none"
TARGETS=(
  "$KERNEL_TARGET"
  "$USERLAND_TARGET"
)
EFI_NAME="mana_os.efi"

if ! command -v rustup >/dev/null 2>&1; then
  echo "[ERROR] rustup is required for reproducible target setup!!"
  exit 1
fi

for TARGET in "${TARGETS[@]}"; do
  if ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "[SETUP] Installing RUST target: $TARGET"
    rustup target add "$TARGET"
  fi
done

echo "[BUILD] Compiling the KERNEL..."
cargo build --target "$KERNEL_TARGET"

echo "[BUILD] Setting up ESP..."
mkdir -p esp/EFI/BOOT
cp "target/$KERNEL_TARGET/debug/$EFI_NAME" esp/EFI/BOOT/BOOTX64.EFI

if [ ! -f disk.img ]; then
  echo "[BUILD] Creating disk.img..."
  dd if=/dev/zero of=disk.img bs=1M count=64
fi

if [ ! -f OVMF.fd ]; then
  echo "[ERROR] Missing OVMF.fd. Install/copy OVMF firmware into the repo root."
  exit 1
fi

echo "[RUN] Starting QEMU..."
qemu-system-x86_64 \
  -display gtk,zoom-to-fit=on \
  -drive if=pflash,format=raw,readonly=on,file=OVMF.fd \
  -drive format=raw,file=fat:rw:esp \
  -drive file=disk.img,if=none,id=drive0,format=raw \
  -device ahci,id=ahci0 \
  -device ide-hd,drive=drive0,bus=ahci0.0 \
  -serial stdio