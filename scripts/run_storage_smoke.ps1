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
    "Frame allocator self-checks passed: .*released_frame_reuse=true",
    "Kernel virtual range allocator self-checks passed: non_overlapping_reuse=true exhaustion_rejection=true",
    "Dynamic kernel mapping lifecycle self-check passed: map=true unmap=true virtual_reuse=true physical_reuse=true",
    "User address-space template self-check passed: kernel_shared=true user_window_empty=true",
    "User address-space reclaim self-check passed: user_pages=1 page_table_pages=4",
    "Loading user ELF from filesystem: path=/disk/bin/smoke_demo",
    "User address space prepared: pml4=",
    "ELF segment mapped: .*perms=R-X",
    "ELF segment mapped: .*perms=R--",
    "ELF segment mapped: .*perms=RW-",
    "User stack mapping verified: pages=4 .*guard_unmapped=true",
    "User task spawned\. task_id=.* address_space=",
    "Installed user task kernel stack: task=.* address_space=",
    "User entry arguments prepared: argc=2",
    "Multi-user smoke tasks spawned",
    "Multi-user active set prepared: tasks=2",
    "User timer trap frame saved",
    "User task preempted by timer",
    "User task entered from timer context",
    "User task resumed from timer context",
    "fstat -> fd=",
    "getdents64 -> fd=",
    "user entry arguments ok",
    "user shell ok",
    "user bss ok",
    "user smoke ok",
    "User exit preemption window closed: task=.*",
    "User task exited: code=0",
    "Restored kernel address space after user exit",
    "User address space reclaimed: task=.* user_pages=.* page_table_pages=.*",
    "User kernel stack reclaimed: task=.* writable_pages=4 virtual_pages=5",
    "User task resources reclaimed: task=.* address_space=true kernel_stack=true",
    "Active user lifecycle drained: exits=2",
    "Multi-user preemption smoke passed: tasks=2",
    "Scheduler diagnostics verified: .*user_tasks=2 .*finished=2 .*active_user_tasks=0 .*active_user_address_spaces=0 .*pending_user_exits=0 .*preemption_enabled=true .*user_exit_preemption_window_closes=2 .*user_exit_return_stack_sets=2 .*user_exit_return_stack_takes=2 .*reclaimed_user_resource_records=2 .*reclaimed_user_kernel_stacks=2 .*reclaimed_kernel_stack_writable_pages=8 .*reclaimed_kernel_stack_virtual_pages=10 .*timer_preemptions=.* user_resumes=.* finished_tasks=2",
    "Scheduler task snapshots verified: rows=4 finished_user_tasks=2 fully_reclaimed_user_tasks=2",
    "Tasks command smoke passed: command=`"tasks`" output_lines=9",
    "Console status strip smoke passed"
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
