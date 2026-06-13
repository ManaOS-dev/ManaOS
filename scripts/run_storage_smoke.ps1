param(
    [int]$TimeoutSeconds = 30,
    [string]$SerialLog = "storage-smoke-serial.log"
)

$ErrorActionPreference = "Stop"

if (Test-Path $SerialLog) {
    Remove-Item -LiteralPath $SerialLog -Force
}

New-Item -ItemType Directory -Force -Path "esp\EFI\BOOT" | Out-Null
Copy-Item -LiteralPath "target\x86_64-unknown-uefi\debug\mana_os.efi" `
    -Destination "esp\EFI\BOOT\BOOTX64.EFI" -Force

$arguments = @(
    "-display", "none",
    "-no-reboot",
    "-drive", "if=pflash,format=raw,readonly=on,file=OVMF.fd",
    "-drive", "format=raw,file=fat:rw:esp",
    "-drive", "file=disk.img,if=none,id=drive0,format=raw",
    "-device", "ahci,id=ahci0",
    "-device", "ide-hd,drive=drive0,bus=ahci0.0",
    "-serial", "file:$SerialLog",
    "-monitor", "none"
)

$qemu = Start-Process -FilePath "qemu-system-x86_64" `
    -ArgumentList $arguments `
    -PassThru `
    -WindowStyle Hidden

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
$expectedPatterns = @(
    "Persistent block-device service registered",
    "Registered block device",
    "Backup boot sector validated",
    "FSInfo: sector=1",
    "Registered FAT32 file backend for virtual filesystem",
    "Registered FAT32 file backend for virtual filesystem: path=/disk/bin/smoke_demo",
    "Disk file smoke read",
    "Pipeline command smoke passed",
    "ManaOS Kernel is alive\.",
    "Invalid ELF rejection smoke passed",
    "Loading user ELF from filesystem: path=/disk/bin/smoke_demo",
    "ELF segment mapped: .*perms=R-X",
    "ELF segment mapped: .*perms=R--",
    "ELF segment mapped: .*perms=RW-",
    "User stack mapping verified: pages=4 guard_unmapped=true",
    "User entry arguments prepared: argc=2",
    "User timer trap frame saved",
    "User task preempted by timer",
    "User task resumed from timer context",
    "fstat -> fd=",
    "getdents64 -> fd=",
    "user entry arguments ok",
    "user shell ok",
    "user bss ok",
    "user smoke ok",
    "User task exited: code=0"
)

try {
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
        if ($qemu.HasExited) {
            break
        }
        if (Test-Path $SerialLog) {
            $allFound = $true
            foreach ($pattern in $expectedPatterns) {
                if (-not (Select-String -Path $SerialLog -Pattern $pattern -Quiet)) {
                    $allFound = $false
                    break
                }
            }
            if ($allFound) {
                Write-Host "[storage-smoke] PASS"
                foreach ($pattern in $expectedPatterns) {
                    Select-String -Path $SerialLog -Pattern $pattern | Select-Object -First 1
                }
                exit 0
            }
        }
    }

    Write-Host "[storage-smoke] FAIL"
    if (Test-Path $SerialLog) {
        Get-Content $SerialLog -Tail 80
    }
    exit 1
}
finally {
    if (-not $qemu.HasExited) {
        Stop-Process -Id $qemu.Id -Force
        $qemu.WaitForExit()
    }
}
