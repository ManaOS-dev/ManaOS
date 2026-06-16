//! Kernel boot smoke checks and summary state collection.

use crate::kernel;
use crate::kernel::diagnostic::summary::{BootSummary, CheckStatus};

/// Verify kernel stack guard fault diagnostics.
pub fn verify_kernel_stack_guard_fault_diagnostics() {
    let diagnostic = kernel::task::get_kernel_stack_guard_fault_diagnostic_sample()
        .expect("kernel stack guard diagnostics must classify a scheduler-owned stack");
    crate::log_info!(
        "fault",
        "Kernel stack guard diagnostics verified: owner={} task={} guard={:#x} writable_start={:#x} stack_top={:#x}",
        diagnostic.owner().as_str(),
        diagnostic.task_identifier(),
        diagnostic.guard_page_start(),
        diagnostic.writable_start(),
        diagnostic.stack_top()
    );
}

/// Verify the in-kernel filesystem smoke paths.
pub fn verify_kernel_filesystem() {
    crate::log_info!("fs", "Standard output is connected to /dev/console.");
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_OUTPUT, b"");
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_ERROR, b"");

    kernel::filesystem::mount_ram_file("/hello.txt", b"hello from ramfs\n");
    kernel::filesystem::mount_read_only_file("/docs/manual-smoke.txt", b"cat /disk/hello.txt\n");
    let descriptor =
        kernel::filesystem::open("/hello.txt").expect("ramfs smoke test file must open");
    let mut buffer = [0_u8; 32];
    kernel::filesystem::seek(descriptor, 0).expect("ramfs smoke test seek must succeed");
    let bytes_read =
        kernel::filesystem::read(descriptor, &mut buffer).expect("ramfs smoke test must read");
    kernel::filesystem::close(descriptor).expect("ramfs smoke test descriptor must close");
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_OUTPUT, &buffer[..bytes_read]);

    let dev_entries =
        kernel::filesystem::list_directory("/dev").expect("/dev listing must be available");
    crate::log_info!(
        "fs",
        "VFS directory listing smoke: path=/dev entries={}",
        dev_entries.len()
    );
    let dev_descriptor = kernel::filesystem::open("/dev").expect("/dev directory handle must open");
    let dev_metadata =
        kernel::filesystem::descriptor_metadata(dev_descriptor).expect("/dev stat must succeed");
    let mut directory_entry_count = 0_usize;
    while kernel::filesystem::read_directory(dev_descriptor)
        .expect("/dev readdir must succeed")
        .is_some()
    {
        directory_entry_count += 1;
    }
    kernel::filesystem::close(dev_descriptor).expect("/dev descriptor must close");
    crate::log_info!(
        "fs",
        "VFS directory handle smoke: path=/dev entries={} type={:?}",
        directory_entry_count,
        dev_metadata.file_type
    );

    let null_descriptor =
        kernel::filesystem::open("/dev/null").expect("null device must open during smoke test");
    let _ = kernel::filesystem::write(null_descriptor, b"discarded");
    kernel::filesystem::close(null_descriptor).expect("null descriptor must close");

    let _ = kernel::filesystem::read(kernel::filesystem::STANDARD_INPUT, &mut buffer);
}

/// Verify frame allocator self-check rules.
pub fn verify_frame_allocator_rules() -> bool {
    let zero_skip_ok =
        kernel::memory::frame_allocator::verify_zero_address_skip_for_multi_frame_allocations();
    let range_tracking_ok =
        kernel::memory::frame_allocator::verify_reserved_used_and_free_range_tracking();
    let duplicate_allocation_ok =
        kernel::memory::frame_allocator::verify_duplicate_allocation_rejection();
    let contiguous_boundaries_ok =
        kernel::memory::frame_allocator::verify_contiguous_allocation_boundaries();
    let reserved_exclusion_ok = kernel::memory::frame_allocator::verify_reserved_range_exclusion();
    let owner_tracking_ok = kernel::memory::frame_allocator::verify_owner_tracking();
    let released_frame_reuse_ok = kernel::memory::frame_allocator::verify_released_frame_reuse();
    let typed_frame_start_ok = kernel::memory::frame_allocator::verify_typed_physical_frame_start();
    let owner_coverage_ok = kernel::memory::frame_allocator::verify_explicit_owner_coverage();
    let passed = zero_skip_ok
        && range_tracking_ok
        && duplicate_allocation_ok
        && contiguous_boundaries_ok
        && reserved_exclusion_ok
        && owner_tracking_ok
        && released_frame_reuse_ok
        && typed_frame_start_ok
        && owner_coverage_ok;
    if passed {
        crate::log_info!(
            "memory",
            "Frame allocator self-checks passed: zero_skip=true range_tracking=true duplicate_allocation=true contiguous_boundaries=true reserved_exclusion=true owner_tracking=true released_frame_reuse=true typed_frame_start=true owner_coverage=true"
        );
    } else {
        crate::log_error!(
            "memory",
            "Frame allocator self-checks failed: zero_skip={} range_tracking={} duplicate_allocation={} contiguous_boundaries={} reserved_exclusion={} owner_tracking={} released_frame_reuse={} typed_frame_start={} owner_coverage={}",
            zero_skip_ok,
            range_tracking_ok,
            duplicate_allocation_ok,
            contiguous_boundaries_ok,
            reserved_exclusion_ok,
            owner_tracking_ok,
            released_frame_reuse_ok,
            typed_frame_start_ok,
            owner_coverage_ok
        );
    }
    passed
}

/// Verify typed memory address wrapper self-check rules.
pub fn verify_memory_address_wrapper_rules() -> bool {
    let user_virtual_address_ok = kernel::memory::address::verify_typed_user_virtual_address();
    let user_page_start_ok = kernel::memory::address::verify_typed_user_page_start();
    let frame_count_ok = kernel::memory::address::verify_typed_frame_count();
    let passed = user_virtual_address_ok && user_page_start_ok && frame_count_ok;
    if passed {
        crate::log_info!(
            "memory",
            "Memory address wrapper self-checks passed: user_virtual_address=true user_page_start=true frame_count=true"
        );
    } else {
        crate::log_error!(
            "memory",
            "Memory address wrapper self-checks failed: user_virtual_address={} user_page_start={} frame_count={}",
            user_virtual_address_ok,
            user_page_start_ok,
            frame_count_ok
        );
    }
    passed
}

/// Verify the kernel virtual range allocator self-check rules.
pub fn verify_kernel_virtual_range_allocator_rules() {
    let non_overlapping_reuse_ok =
        kernel::memory::virtual_allocator::verify_kernel_virtual_range_allocation();
    let exhaustion_rejection_ok =
        kernel::memory::virtual_allocator::verify_kernel_virtual_range_exhaustion();

    if non_overlapping_reuse_ok && exhaustion_rejection_ok {
        crate::log_info!(
            "memory",
            "Kernel virtual range allocator self-checks passed: non_overlapping_reuse=true exhaustion_rejection=true"
        );
    } else {
        crate::log_error!(
            "memory",
            "Kernel virtual range allocator self-checks failed: non_overlapping_reuse={} exhaustion_rejection={}",
            non_overlapping_reuse_ok,
            exhaustion_rejection_ok
        );
    }
}

/// Verify dynamic kernel mapping lifecycle behavior.
pub fn verify_dynamic_kernel_mapping_lifecycle(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    assert!(
        kernel::memory::paging::verify_kernel_dynamic_mapping_lifecycle(frame_allocator),
        "dynamic kernel mapping lifecycle smoke must pass"
    );
    crate::log_info!(
        "memory",
        "Dynamic kernel mapping lifecycle self-check passed: map=true unmap=true virtual_reuse=true physical_reuse=true"
    );
}

/// Verify user address-space template behavior.
pub fn verify_user_address_space_template(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    assert!(
        kernel::memory::address_space::verify_user_address_space_template(
            frame_allocator,
            verify_kernel_filesystem as *const () as usize,
        ),
        "user address-space template smoke must pass"
    );
    crate::log_info!(
        "memory",
        "User address-space template self-check passed: kernel_shared=true user_window_empty=true"
    );
}

/// Verify user address-space reclaim behavior.
pub fn verify_user_address_space_reclaim(
    frame_allocator: &mut kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    let reclaim = kernel::memory::address_space::verify_user_address_space_reclaim(frame_allocator)
        .expect("user address-space reclaim smoke must pass");
    crate::log_info!(
        "memory",
        "User address-space reclaim self-check passed: user_pages={} page_table_pages={}",
        reclaim.user_pages(),
        reclaim.page_table_pages()
    );
}

/// Verify ELF loader rejection self-check rules.
pub fn verify_elf_loader_rules() {
    assert!(
        kernel::elf::verify_invalid_elf_rejections(),
        "ELF invalid-image rejection smoke must pass"
    );
}

/// Verify primary storage can read multiple sectors.
pub fn verify_primary_storage_device() {
    let Some(data_address) = kernel::driver::storage::get_primary_data_address() else {
        return;
    };

    if kernel::driver::storage::read_primary_blocks(0, 2, data_address) {
        crate::log_info!(
            "storage",
            "Primary block device multi-sector read smoke passed."
        );
    } else {
        crate::log_warn!(
            "storage",
            "Primary block device multi-sector read smoke failed."
        );
    }
}

/// Record storage boot summary state and run disk filesystem smoke checks.
pub fn record_storage_boot_summary(boot_summary: &mut BootSummary) {
    let storage_devices = kernel::driver::storage::get_storage_devices();
    let selected_partition = kernel::driver::storage::get_selected_partition();
    let detected_files = kernel::driver::storage::get_detected_files();
    boot_summary.ahci = CheckStatus::from_bool(!storage_devices.is_empty());
    boot_summary.gpt = CheckStatus::from_bool(selected_partition.is_some());
    boot_summary.fat32 = CheckStatus::from_bool(!detected_files.is_empty());
    boot_summary.mounted_files = Some(detected_files.len());

    let mut filesystem_smoke_passed = false;
    if mount_detected_disk_files() {
        verify_mounted_disk_file("/disk/hello.txt");
        filesystem_smoke_passed = verify_kernel_console_pipeline();
    }
    boot_summary.smoke_tests.filesystem = CheckStatus::from_bool(filesystem_smoke_passed);
}

fn verify_mounted_disk_file(path: &str) {
    let descriptor = kernel::filesystem::open(path).expect("mounted disk file must open");
    let mut buffer = [0_u8; 64];
    let bytes_read =
        kernel::filesystem::read(descriptor, &mut buffer).expect("mounted disk file must read");
    kernel::filesystem::close(descriptor).expect("mounted disk file descriptor must close");
    crate::log_info!(
        "fs",
        "Disk file smoke read: path={} bytes={}",
        path,
        bytes_read
    );
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_OUTPUT, &buffer[..bytes_read]);
}

fn verify_kernel_console_pipeline() -> bool {
    const PIPELINE_COMMAND: &str = "cat /disk/hello.txt | grep FAT32";

    match kernel::console::verify_pipeline_smoke(PIPELINE_COMMAND) {
        Some(output_lines) if output_lines > 0 => {
            crate::log_info!(
                "console",
                "Pipeline command smoke passed: command=\"{}\" output_lines={}",
                PIPELINE_COMMAND,
                output_lines
            );
            true
        }
        _ => {
            crate::log_warn!(
                "console",
                "Pipeline command smoke failed: command=\"{}\"",
                PIPELINE_COMMAND
            );
            false
        }
    }
}

fn mount_detected_disk_files() -> bool {
    let mut hello_mounted = false;
    for file in kernel::driver::storage::get_detected_files() {
        kernel::filesystem::mount_fat32_file(
            &file.mount_path,
            file.size,
            file.backend_index,
            kernel::driver::storage::read_detected_file_range,
        );
        crate::log_info!(
            "fs",
            "Mounted disk file: path={} bytes={}",
            file.mount_path,
            file.size
        );
        if file.mount_path == "/disk/hello.txt" {
            hello_mounted = true;
        }
    }
    hello_mounted
}

/// Record scheduler boot summary state after userspace smoke tests.
pub fn record_scheduler_boot_summary(boot_summary: &mut BootSummary) {
    if let Some(diagnostics) = kernel::task::get_scheduler_diagnostics() {
        boot_summary.user_tasks_spawned = Some(diagnostics.user_tasks());
        boot_summary.user_tasks_exited = Some(diagnostics.finished_tasks());
        boot_summary.user_resources_freed = CheckStatus::from_bool(
            diagnostics.reclaimed_user_resource_records() == diagnostics.user_tasks()
                && diagnostics.reclaimed_user_address_spaces() == diagnostics.user_tasks()
                && diagnostics.reclaimed_user_kernel_stacks() == diagnostics.user_tasks()
                && diagnostics.active_user_address_spaces() == 0,
        );
        boot_summary.smoke_tests.scheduler = CheckStatus::Pass;
        boot_summary.smoke_tests.preemption = CheckStatus::from_bool(
            diagnostics.preemption_enabled() && diagnostics.timer_preemptions() > 0,
        );
    }
}

/// Record console smoke summary state after userspace smoke tests.
pub fn record_console_smoke_summary(boot_summary: &mut BootSummary) {
    let scheduler_console_passed = kernel::diagnostic::smoke::verify_scheduler_console_command();
    let memory_console_passed = kernel::diagnostic::smoke::verify_memory_console_command();
    let syscall_console_passed = kernel::diagnostic::smoke::verify_syscall_trace_console_command();
    let status_strip_passed = kernel::diagnostic::smoke::verify_console_status_strip();
    boot_summary.smoke_tests.scheduler = CheckStatus::from_bool(
        boot_summary.smoke_tests.scheduler == CheckStatus::Pass
            && scheduler_console_passed
            && memory_console_passed
            && status_strip_passed,
    );
    boot_summary.smoke_tests.syscall = CheckStatus::from_bool(syscall_console_passed);
}
