@echo off
setlocal
chcp 65001<nul

echo [build] Compiling kernel...
cargo build --target x86_64-unknown-uefi
if %errorlevel% neq 0 (
    echo [error] Build failed.
    pause
    exit /b %errorlevel%
)

echo [build] Setting up ESP...
if not exist esp\EFI\BOOT mkdir esp\EFI\BOOT
copy target\x86_64-unknown-uefi\debug\mana_os.efi esp\EFI\BOOT\BOOTX64.EFI /y

echo [run] Starting QEMU...
qemu-system-x86_64 ^
  -drive if=pflash,format=raw,readonly=on,file=OVMF.fd ^
  -drive format=raw,file=fat:rw:esp ^
  -chardev stdio,id=char0,mux=on ^
  -serial chardev:char0 ^
  -mon chardev=char0,mode=readline

endlocal