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
    "ACPI parser self-check passed: rsdp=true root_table=true madt=true",
    "ACPI root table verified",
    "source\s+= uefi_acpi[12]",
    "root_table\s+= (xsdt|rsdt)",
    "root_address\s+= 0x[0-9a-f]+",
    "entries\s+= [1-9][0-9]*",
    "checksum\s+= true",
    "ACPI MADT verified",
    "local_apic\s+= 0x[0-9a-f]+",
    "pc_at_compatible\s+= (true|false)",
    "ioapics\s+= [1-9][0-9]*",
    "ACPI interrupt topology verified",
    "retained_local_apics\s+= [1-9][0-9]*",
    "retained_ioapics\s+= [1-9][0-9]*",
    "topology_truncated\s+= false",
    "ioapic0_address\s+= 0x[0-9a-f]+",
    "legacy_irq0_flags\s+= 0x[0-9a-f]+",
    "x2apic0_present\s+= (true|false)",
    "APIC routing provider configured",
    "local_apic_supported\s+= true",
    "local_apic_address\s+= 0x[0-9a-f]+",
    "local_apic_enabled\s+= true",
    "ioapic_address\s+= 0x[0-9a-f]+",
    "legacy_irq_routes\s+= [1-9][0-9]*",
    "route_truncated\s+= false",
    "Interrupt controller backend initialized: legacy_pic_initialized=false legacy_fallback_enabled=false legacy_pic_masked_for_apic=true master_mask=0xff slave_mask=0xff",
    "IOAPIC redirection plan verified",
    "entries\s+= 3",
    "truncated\s+= false",
    "timer_vector\s+= 32",
    "timer_masked\s+= false",
    "keyboard_vector\s+= 33",
    "mouse_vector\s+= 44",
    "Local APIC MMIO mapped: address=0x[0-9a-f]+ size=4096",
    "Local APIC EOI provider verified: configured=true routing_active=false software_enabled=(true|false) local_apic_address=0x[0-9a-f]+ local_apic_id=.* version=0x[0-9a-f]+ max_lvt_entry=[1-9][0-9]* spurious_vector=0x[0-9a-f]+",
    "IOAPIC MMIO mapped: address=0x[0-9a-f]+ size=4096",
    "IOAPIC redirection staging verified",
    "staged\s+= 3",
    "readback_matches\s+= true",
    "masked\s+= true",
    "out_of_range_entries\s+= 0",
    "IOAPIC routing activated: entries=3 activated=3 readback_matches=true routing_active=true masked=false local_apic_software_enabled=true legacy_pic_masked=true out_of_range_entries=0 timer_low_readback=0x[0-9a-f]+ timer_high_readback=0x[0-9a-f]+ keyboard_low_readback=0x[0-9a-f]+ keyboard_high_readback=0x[0-9a-f]+ mouse_low_readback=0x[0-9a-f]+ mouse_high_readback=0x[0-9a-f]+ apic_eoi_count=0 legacy_eoi_count=0",
    "Local APIC timer calibration started: configured=true armed=true masked=true address=0x[0-9a-f]+ vector=32 divide=16 lvt_timer=0x[0-9a-f]+ divide_config=0x3 initial_count=[1-9][0-9]* current_count=[0-9]+ start_ticks=[0-9]+",
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
    "User task sleep requested: task=.* wake_tick=",
    "User task blocked for sleep: task=.* wake_tick=",
    "User task sleep woke: task=.* wake_tick=.* current_tick=",
    "fstat -> fd=",
    "getdents64 -> fd=",
    "brk -> task=.* mapped_pages=2",
    "User heap pages unmapped: .*pages=1",
    "brk -> task=.* mapped_pages=1",
    "mmap -> task=.* pages=3 .*placement=any",
    "User anonymous mapping unmapped: .*pages=1 records=2 active_pages=2",
    "munmap -> task=.* pages=1 unmapped=true active_pages=2 active_records=2",
    "User anonymous mapping unmapped: .*pages=1 records=1 active_pages=1",
    "munmap -> task=.* pages=1 unmapped=true active_pages=1 active_records=1",
    "User anonymous mapping unmapped: .*pages=1 records=0 active_pages=0",
    "munmap -> task=.* pages=1 unmapped=true active_pages=0 active_records=0",
    "mmap -> task=.* requested=0x600000008000 start=0x600000008000 length=4096 pages=1 .*flags=0x100022 placement=fixed_noreplace .*active_pages=1",
    "User mapping fixed replacement prepared: start=0x600000008000 pages=1 records=0 active_pages=0",
    "mmap -> task=.* requested=0x600000008000 start=0x600000008000 length=4096 pages=1 .*flags=0x32 placement=fixed_replace .*active_pages=1",
    "munmap -> task=.* start=0x600000008000 length=4096 pages=1 unmapped=true active_pages=0 active_records=0",
    "mmap -> task=.* length=4096 pages=1 protection=0x1 flags=0x2 placement=any source=file_private active_pages=1 file_private_records=1",
    "mmap file preload -> fd=.* offset=0 start=.* length=4096 bytes=.*",
    "User file_private mapping unmapped: .*pages=1 records=0 active_pages=0",
    "getppid -> parent=0",
    "user process ids ok",
    "user execve abi ok",
    "user syscall errors ok",
    "user entry arguments ok",
    "user sleep ok",
    "user shell ok",
    "user bss ok",
    "user heap ok",
    "user mmap ok",
    "user file mmap ok",
    "user smoke ok",
    "APIC EOI diagnostics verified: routing_active=true apic_eoi_count=[1-9][0-9]* legacy_eoi_count=0",
    "Interrupt vector diagnostics verified: spurious_vector=255 spurious_count=0 unexpected_external_count=0",
    "Local APIC timer calibration verified: configured=true armed=true masked=true decremented=true expired=false address=0x[0-9a-f]+ vector=32 divide=16 start_ticks=[0-9]+ current_ticks=[1-9][0-9]* elapsed_ticks=[1-9][0-9]* initial_count=[1-9][0-9]* current_count=[0-9]+ elapsed_counts=[1-9][0-9]* counts_per_tick=[1-9][0-9]* lvt_timer=0x[0-9a-f]+ divide_config=0x3",
    "IOAPIC timer route masked for Local APIC timer: routing_active=true readback_matches=true masked=true timer_gsi=.* table_index=.* low_register=0x[0-9a-f]+ high_register=0x[0-9a-f]+ low_readback=0x[0-9a-f]+ high_readback=0x[0-9a-f]+",
    "Local APIC timer activated: configured=true running=true masked=false periodic=true address=0x[0-9a-f]+ vector=32 divide=16 activation_ticks=[1-9][0-9]* current_ticks=[1-9][0-9]* initial_count=[1-9][0-9]* current_count=[0-9]+ calibration_counts_per_tick=[1-9][0-9]* lvt_timer=0x[0-9a-f]+ divide_config=0x3",
    "Local APIC timer tick source verified: configured=true running=true masked=false periodic=true address=0x[0-9a-f]+ vector=32 divide=16 activation_ticks=[1-9][0-9]* current_ticks=[1-9][0-9]* elapsed_ticks=[1-9][0-9]* initial_count=[1-9][0-9]* current_count=[0-9]+ calibration_counts_per_tick=[1-9][0-9]* lvt_timer=0x[0-9a-f]+ divide_config=0x3",
    "Local APIC timer post-smoke verified: configured=true running=true masked=false periodic=true address=0x[0-9a-f]+ vector=32 divide=16 activation_ticks=[1-9][0-9]* current_ticks=[1-9][0-9]* elapsed_ticks=[1-9][0-9]* initial_count=[1-9][0-9]* current_count=[0-9]+ calibration_counts_per_tick=[1-9][0-9]* lvt_timer=0x[0-9a-f]+ divide_config=0x3",
    "User return preemption window closed: task=.*",
    "User task exited: code=0",
    "User task exit status retained: parent=0 child=.* code=0 waitable=true",
    "Restored kernel address space after user stop",
    "User address space reclaimed: task=.* user_pages=.* page_table_pages=.*",
    "User kernel stack reclaimed: task=.* writable_pages=4 virtual_pages=5",
    "User task resources reclaimed: task=.* address_space=true kernel_stack=true",
    "Active user lifecycle drained: exits=2",
    "Waitable child exit collected: parent=0 child=.* code=0",
    "Bootstrap child wait collection verified: parent=0 children=2",
    "Multi-user preemption smoke passed: tasks=2",
    "Scheduler diagnostics verified",
    "user_tasks\s+= 2",
    "active_user_tasks\s+= 0",
    "active_user_address_spaces\s+= 0",
    "pending_user_exits\s+= 0",
    "retained_user_exit_statuses\s+= 2",
    "waitable_user_exit_statuses\s+= 0",
    "collected_user_exit_statuses\s+= 2",
    "preemption_state\s+= enabled",
    "preemption_enabled\s+= true",
    "user_sleep_blocks\s+= 2",
    "user_sleep_wakes\s+= 2",
    "reclaimed_user_resource_records\s+= 2",
    "reclaimed_user_address_spaces\s+= 2",
    "reclaimed_user_pages\s+= 22",
    "reclaimed_user_page_table_pages\s+= 20",
    "reclaimed_user_kernel_stacks\s+= 2",
    "reclaimed_kernel_stack_writable_pages\s+= 8",
    "reclaimed_kernel_stack_virtual_pages\s+= 10",
    "timer_user_entries\s+= [1-9][0-9]*",
    "finished_tasks\s+= 2",
    "Scheduler task snapshots verified",
    "rows\s+= 4",
    "finished_user_tasks\s+= 2",
    "fully_reclaimed_user_tasks\s+= 2",
    "released_mmap_snapshots\s+= 2",
    "Frame allocator diagnostics snapshot",
    "page_table\s+= [0-9]+",
    "user_stack\s+= 0",
    "user_elf\s+= 0",
    "user_heap\s+= 0",
    "user_mapping\s+= 0",
    "dynamic_kernel_mapping\s+= 0",
    "ahci_dma\s+= [0-9]+",
    "Tasks command smoke passed: command=`"tasks`" output_lines=15",
    "Memory command smoke passed: command=`"memory`" output_lines=3",
    "Syscall trace: record=1 task=.* number=39 result=0x",
    "Syscall trace controls smoke passed: command=`"syscalls trace`" records=1",
    "Console status strip smoke passed",
    "ManaOS Boot Summary",
    "SYSTEM HEALTHY"
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
