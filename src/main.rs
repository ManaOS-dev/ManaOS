//! `ManaOS` kernel and UEFI entry point.

#![no_main]
#![no_std]
#![feature(abi_x86_interrupt)]
#![deny(missing_docs)]
#![deny(clippy::missing_safety_doc)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate)]

extern crate alloc;

mod arch;
mod kernel;

use alloc::vec::Vec;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::{
    boot,
    mem::memory_map::{MemoryDescriptor, MemoryMap, MemoryType},
};

extern "C" fn idle_task() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

/// Panic handler: dump to serial and halt.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    crate::serial_println!("[PANIC] {}", info);
    arch::hlt_loop();
}

/// Load a file from the EFI System Partition into memory (Boot Phase).
fn load_file(path: &str) -> &'static mut [u8] {
    use uefi::proto::media::file::FileInfo;

    let fs_handle = boot::get_handle_for_protocol::<SimpleFileSystem>()
        .expect("Failed to get SimpleFileSystem handle");
    let mut fs = boot::open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
        .expect("Failed to open SimpleFileSystem");

    let mut root = fs.open_volume().expect("Failed to open volume");

    let mut path_buffer = [0u16; 128];
    let path_cstr = uefi::CStr16::from_str_with_buf(path, &mut path_buffer)
        .expect("Failed to convert path to CStr16");

    let mut file = root
        .open(path_cstr, FileMode::Read, FileAttribute::empty())
        .expect("Failed to open file")
        .into_regular_file()
        .expect("Not a regular file");

    let mut info_buf = [0u8; 256];
    let info = file
        .get_info::<FileInfo>(&mut info_buf)
        .expect("Failed to get file info");
    let size = usize::try_from(info.file_size()).expect("File too large");

    // Allocate memory from UEFI pool
    let ptr = boot::allocate_pool(MemoryType::LOADER_DATA, size)
        .expect("Failed to allocate pool for file");

    // SAFETY: allocate_pool returned a valid pointer to a LOADER_DATA buffer of
    // exactly size bytes, and the buffer remains owned by the boot phase.
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr.as_ptr(), size) };
    file.read(buffer).expect("Failed to read file");

    buffer
}

fn get_framebuffer_info() -> kernel::driver::display::framebuffer::FrameBufferInfo {
    let graphics_output_handle = boot::get_handle_for_protocol::<GraphicsOutput>()
        .expect("GraphicsOutput handle is required for ManaOS framebuffer setup");
    let mut graphics_output =
        boot::open_protocol_exclusive::<GraphicsOutput>(graphics_output_handle)
            .expect("GraphicsOutput protocol is required for ManaOS framebuffer setup");
    kernel::driver::display::framebuffer::get_info(&mut graphics_output)
}

fn import_boot_memory_map<'a>(
    frame_allocator: &mut kernel::memory::frame_allocator::BumpFrameAllocator,
    memory_descriptors: impl Iterator<Item = &'a MemoryDescriptor>,
) {
    for descriptor in memory_descriptors {
        if descriptor.ty == MemoryType::CONVENTIONAL {
            frame_allocator.add_region(
                kernel::memory::address::PhysAddr::new(descriptor.phys_start),
                descriptor.page_count,
            );
        } else {
            frame_allocator.reserve_region_for(
                kernel::memory::address::PhysAddr::new(descriptor.phys_start),
                descriptor.page_count,
                boot_memory_owner_for(descriptor.ty),
            );
        }
    }

    let owner_statistics = frame_allocator.owner_statistics();
    crate::log_info!(
        "memory",
        "Boot memory owner import: free={} firmware_reserved={} kernel_image={} mmio={}",
        owner_statistics.free,
        owner_statistics.firmware_reserved,
        owner_statistics.kernel_image,
        owner_statistics.mmio
    );
}

fn boot_memory_owner_for(
    memory_type: MemoryType,
) -> kernel::memory::frame_allocator::FrameRangeOwner {
    match memory_type {
        MemoryType::LOADER_CODE => kernel::memory::frame_allocator::FrameRangeOwner::KernelImage,
        MemoryType::MMIO | MemoryType::MMIO_PORT_SPACE => {
            kernel::memory::frame_allocator::FrameRangeOwner::Mmio
        }
        _ => kernel::memory::frame_allocator::FrameRangeOwner::FirmwareReserved,
    }
}

fn get_framebuffer_size(
    framebuffer_info: kernel::driver::display::framebuffer::FrameBufferInfo,
) -> u64 {
    (framebuffer_info.stride * framebuffer_info.vertical_resolution * 4) as u64
}

fn allocate_backbuffer(
    frame_allocator: &mut kernel::memory::frame_allocator::BumpFrameAllocator,
    framebuffer_size: u64,
) -> kernel::memory::address::KernelVirtualAddress {
    let backbuffer_pages = framebuffer_size.div_ceil(4096);
    let backbuffer_physical_range = frame_allocator
        .allocate_frames_for(
            backbuffer_pages,
            kernel::memory::frame_allocator::FrameRangeOwner::FramebufferBackbuffer,
        )
        .expect("OOM: failed to allocate framebuffer backbuffer");
    backbuffer_physical_range
        .start()
        .as_identity_mapped_kernel_address()
}

fn initialize_scheduler() {
    kernel::task::initialize();
    kernel::task::spawn(idle_task);
    let task_id = kernel::task::get_current_task_id()
        .expect("scheduler must expose a bootstrap task after initialization");
    crate::log_info!("task", "Scheduler initialized. current_task={}", task_id);
}

fn initialize_architecture_and_drivers() {
    arch::init(kernel::interrupt::syscall_entry as *const () as u64);
    arch::x86_64::interrupt_descriptor_table::register_page_fault_reporter(
        kernel::interrupt::process_page_fault,
    );
    kernel::time::register_timer_ticks_provider(
        arch::x86_64::interrupt_descriptor_table::get_ticks,
    );
    kernel::profiler::register_timestamp_counter_provider(arch::x86_64::read_timestamp_counter);
    kernel::task::architecture::register_context_switch(arch::x86_64::switch_context);
    kernel::task::architecture::register_user_mode_entry(arch::x86_64::enter_user_mode);
    kernel::task::architecture::register_returnable_user_mode_entry(
        arch::x86_64::enter_user_mode_once,
    );
    kernel::task::architecture::register_kernel_stack_installer(
        arch::x86_64::global_descriptor_table::set_privilege_stack_top,
    );
    kernel::task::user_mode::register_selectors(kernel::task::user_mode::UserModeSelectors {
        data: arch::x86_64::global_descriptor_table::USER_DATA_SELECTOR,
        code: arch::x86_64::global_descriptor_table::USER_CODE_SELECTOR,
    });
    crate::log_info!("arch", "Architecture initialized.");
    let user_selectors = kernel::task::user_mode::get_selectors();
    crate::log_info!(
        "task",
        "Ring 3 selectors installed. code={:#06x}, data={:#06x}",
        user_selectors.code,
        user_selectors.data
    );

    arch::x86_64::interrupt_descriptor_table::register_processors(
        arch::x86_64::interrupt_descriptor_table::InterruptProcessors {
            timer_tick: kernel::interrupt::process_timer_tick,
            keyboard_byte: kernel::interrupt::push_keyboard_byte,
            mouse_byte: kernel::interrupt::push_mouse_byte,
        },
    );

    crate::log_info!("driver", "Initializing mouse...");
    kernel::driver::input::mouse::init();
    crate::log_info!("driver", "Mouse initialized.");

    arch::x86_64::enable_interrupts();
}

fn verify_kernel_filesystem() {
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

fn verify_frame_allocator_rules() {
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
    let owner_coverage_ok = kernel::memory::frame_allocator::verify_explicit_owner_coverage();
    if zero_skip_ok
        && range_tracking_ok
        && duplicate_allocation_ok
        && contiguous_boundaries_ok
        && reserved_exclusion_ok
        && owner_tracking_ok
        && owner_coverage_ok
    {
        crate::log_info!(
            "memory",
            "Frame allocator self-checks passed: zero_skip=true range_tracking=true duplicate_allocation=true contiguous_boundaries=true reserved_exclusion=true owner_tracking=true owner_coverage=true"
        );
    } else {
        crate::log_error!(
            "memory",
            "Frame allocator self-checks failed: zero_skip={} range_tracking={} duplicate_allocation={} contiguous_boundaries={} reserved_exclusion={} owner_tracking={} owner_coverage={}",
            zero_skip_ok,
            range_tracking_ok,
            duplicate_allocation_ok,
            contiguous_boundaries_ok,
            reserved_exclusion_ok,
            owner_tracking_ok,
            owner_coverage_ok
        );
    }
}

fn verify_kernel_virtual_range_allocator_rules() {
    let monotonic_allocation_ok =
        kernel::memory::virtual_allocator::verify_kernel_virtual_range_allocation();
    let exhaustion_rejection_ok =
        kernel::memory::virtual_allocator::verify_kernel_virtual_range_exhaustion();

    if monotonic_allocation_ok && exhaustion_rejection_ok {
        crate::log_info!(
            "memory",
            "Kernel virtual range allocator self-checks passed: monotonic_allocation=true exhaustion_rejection=true"
        );
    } else {
        crate::log_error!(
            "memory",
            "Kernel virtual range allocator self-checks failed: monotonic_allocation={} exhaustion_rejection={}",
            monotonic_allocation_ok,
            exhaustion_rejection_ok
        );
    }
}

fn verify_elf_loader_rules() {
    assert!(
        kernel::elf::verify_invalid_elf_rejections(),
        "ELF invalid-image rejection smoke must pass"
    );
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

fn verify_kernel_console_pipeline() {
    const PIPELINE_COMMAND: &str = "cat /disk/hello.txt | grep FAT32";

    match kernel::console::verify_pipeline_smoke(PIPELINE_COMMAND) {
        Some(output_lines) if output_lines > 0 => crate::log_info!(
            "console",
            "Pipeline command smoke passed: command=\"{}\" output_lines={}",
            PIPELINE_COMMAND,
            output_lines
        ),
        _ => crate::log_warn!(
            "console",
            "Pipeline command smoke failed: command=\"{}\"",
            PIPELINE_COMMAND
        ),
    }
}

fn verify_primary_storage_device() {
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

fn read_kernel_file(path: &str) -> Option<Vec<u8>> {
    let metadata = kernel::filesystem::metadata(path).ok()?;
    if metadata.file_type != kernel::filesystem::FileType::Regular {
        return None;
    }

    let descriptor = kernel::filesystem::open(path).ok()?;
    let mut contents = Vec::new();
    contents
        .try_reserve_exact(metadata.size)
        .expect("OOM: failed to reserve kernel file buffer");
    contents.resize(metadata.size, 0);

    let mut bytes_read = 0_usize;
    while bytes_read < metadata.size {
        let read_now = kernel::filesystem::read(descriptor, &mut contents[bytes_read..]).ok()?;
        if read_now == 0 {
            break;
        }
        bytes_read = bytes_read
            .checked_add(read_now)
            .expect("kernel file read byte count overflowed");
    }
    kernel::filesystem::close(descriptor).ok()?;
    contents.truncate(bytes_read);
    Some(contents)
}

fn run_user_smoke_demo(frame_allocator: &mut kernel::memory::frame_allocator::BumpFrameAllocator) {
    let user_stack_pages = 4;
    let user_stack_top =
        kernel::memory::user_stack::allocate_user_stack(frame_allocator, user_stack_pages);
    assert!(
        kernel::memory::user_stack::verify_user_stack_mapping(user_stack_pages),
        "user stack mapping and guard page smoke must pass"
    );
    crate::log_info!(
        "memory",
        "User stack mapping verified: pages={} guard_unmapped=true",
        user_stack_pages
    );

    let user_elf_path = "/disk/bin/smoke_demo";
    let user_elf_bytes =
        read_kernel_file(user_elf_path).expect("user smoke ELF must be readable from /disk/bin");
    crate::log_info!(
        "elf",
        "Loading user ELF from filesystem: path={} bytes={}",
        user_elf_path,
        user_elf_bytes.len()
    );
    let user_elf: kernel::elf::LoadedElf =
        kernel::elf::load_user_program(frame_allocator, &user_elf_bytes, user_elf_path);
    let user_entry_point = user_elf.entry_point();
    let user_stack_probe = user_stack_top
        .checked_sub(1)
        .expect("user stack top must be above the mapped stack");
    assert!(
        kernel::memory::paging::verify_kernel_user_mapping_permissions(
            verify_kernel_filesystem as *const () as usize,
            user_stack_probe.as_usize(),
            user_entry_point.as_usize(),
        ),
        "kernel and user mapping permission smoke must pass"
    );
    crate::log_info!(
        "memory",
        "Kernel/user mapping permission self-check passed."
    );
    assert!(
        kernel::memory::paging::verify_syscall_user_data_permissions(
            user_stack_probe.as_usize(),
            user_entry_point.as_usize(),
        ),
        "syscall user data permission smoke must pass"
    );
    crate::log_info!("memory", "Syscall user data permission self-check passed.");
    let user_entry_arguments = [user_elf_path, "--storage-smoke"];
    let user_entry_environment = ["MANAOS_BOOT=storage-smoke"];
    let prepared_user_stack = kernel::memory::user_stack::prepare_initial_stack(
        user_stack_top,
        &user_entry_arguments,
        &user_entry_environment,
    );
    crate::log_info!(
        "task",
        "User entry arguments prepared: argc={} argv={:#x} envp={:#x}",
        prepared_user_stack.argument_count(),
        prepared_user_stack.argument_values_pointer().as_u64(),
        prepared_user_stack.environment_values_pointer().as_u64()
    );

    let user_task_id = kernel::task::spawn_user_task(
        user_entry_point,
        prepared_user_stack.stack_pointer(),
        kernel::task::UserEntryArguments::new(
            prepared_user_stack.argument_count(),
            prepared_user_stack.argument_values_pointer(),
            prepared_user_stack.environment_values_pointer(),
        ),
    );
    crate::log_info!("task", "User task spawned. task_id={}", user_task_id);
    crate::log_info!("task", "User demo started.");
    if let Some(exit_code) = kernel::task::run_user_task_once(user_task_id) {
        crate::log_info!("task", "UI resumed after user exit: code={}", exit_code);
    }
}

#[entry]
fn main() -> Status {
    // ────────────────────────────────────────────────
    // Boot Phase (UEFI Services available)
    // ────────────────────────────────────────────────
    kernel::logger::init();

    log::info!("ManaOS booting (HAL edition)...");

    let framebuffer_info = get_framebuffer_info();

    log::info!("Calling ExitBootServices...");

    // Pre-load fonts before exiting boot services
    let font_inter = load_file("Inter.ttf");
    let font_noto = load_file("NotoSansJP.ttf");

    // ────────────────────────────────────────────────
    // ExitBootServices
    // ────────────────────────────────────────────────
    kernel::logger::disable();
    // SAFETY: All required boot service resources have been acquired and UEFI
    // console logging is disabled before leaving the boot-services phase.
    let mmap = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };

    // ────────────────────────────────────────────────
    // Kernel Phase
    // ────────────────────────────────────────────────
    kernel::serial::init();
    crate::log_info!("serial", "ExitBootServices OK.");
    let mut frame_allocator = kernel::memory::frame_allocator::BumpFrameAllocator::new();
    import_boot_memory_map(&mut frame_allocator, mmap.entries());
    verify_frame_allocator_rules();
    verify_kernel_virtual_range_allocator_rules();
    verify_elf_loader_rules();

    // ────────────────────────────────────────────────
    // Kernel Phase (UEFI Services unavailable)
    // ────────────────────────────────────────────────
    crate::log_info!("kernel", "ManaOS Kernel phase started.");

    let framebuffer_size = get_framebuffer_size(framebuffer_info);
    let backbuffer_address = allocate_backbuffer(&mut frame_allocator, framebuffer_size);

    kernel::boot::initialize(
        &mut frame_allocator,
        mmap.entries(),
        framebuffer_info,
        kernel::driver::display::font::FontAssets {
            inter: font_inter,
            noto: font_noto,
        },
        backbuffer_address,
    );
    kernel::filesystem::initialize();
    crate::log_info!("fs", "Kernel filesystem initialized.");
    verify_kernel_filesystem();
    kernel::driver::storage::init(
        &mut frame_allocator,
        kernel::driver::storage::PciConfigurationAccess::new(
            arch::x86_64::pci_configuration::read_config32,
            arch::x86_64::pci_configuration::write_config32,
        ),
    );
    verify_primary_storage_device();
    if mount_detected_disk_files() {
        verify_mounted_disk_file("/disk/hello.txt");
        verify_kernel_console_pipeline();
    }
    initialize_scheduler();
    initialize_architecture_and_drivers();

    crate::log_info!("kernel", "ManaOS Kernel is alive.");

    // Calibrate TSC for profiling before user tasks can preempt the bootstrap task.
    kernel::profiler::calibrate_tsc();

    kernel::runtime::initialize();

    run_user_smoke_demo(&mut frame_allocator);

    // Main Loop
    loop {
        kernel::runtime::tick();

        // For maximum performance testing, we don't hlt.
        // x86_64::instructions::hlt();
    }
}
