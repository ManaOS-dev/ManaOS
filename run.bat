@echo off
setlocal
chcp 65001>nul

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

if not exist disk.img (
    echo [build] Creating disk.img...
    powershell -NoLogo -NoProfile -Command "$bytes = New-Object byte[] 67108864; [System.IO.File]::WriteAllBytes('disk.img', $bytes)"
)

echo [run] Starting QEMU...
qemu-system-x86_64 ^
  -display gtk,zoom-to-fit=on ^
  -drive if=pflash,format=raw,readonly=on,file=OVMF.fd ^
  -drive format=raw,file=fat:rw:esp ^
  -drive file=disk.img,if=none,id=drive0,format=raw ^
  -device ahci,id=ahci0 ^
  -device ide-hd,drive=drive0,bus=ahci0.0 ^
  -chardev stdio,id=char0,mux=on ^
  -serial chardev:char0 ^
  -mon chardev=char0,mode=readline

endlocal
