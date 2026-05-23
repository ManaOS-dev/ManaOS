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

fn add_conventional_memory_regions<'a>(
    frame_allocator: &mut kernel::memory::frame_allocator::BumpFrameAllocator,
    memory_descriptors: impl Iterator<Item = &'a MemoryDescriptor>,
) {
    for descriptor in memory_descriptors {
        if descriptor.ty == MemoryType::CONVENTIONAL {
            frame_allocator.add_region(descriptor.phys_start, descriptor.page_count);
        }
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
) -> *mut u8 {
    let backbuffer_pages = framebuffer_size.div_ceil(4096);
    let backbuffer_physical_address = frame_allocator
        .allocate_frames(backbuffer_pages)
        .expect("OOM: failed to allocate framebuffer backbuffer");
    backbuffer_physical_address as *mut u8
}

fn initialize_scheduler() {
    kernel::task::initialize();
    kernel::task::spawn(idle_task);
    let task_id = kernel::task::get_current_task_id()
        .expect("scheduler must expose a bootstrap task after initialization");
    crate::serial_println!("[ok   ] Scheduler initialized. current task: {}", task_id);
}

fn initialize_architecture_and_drivers() {
    arch::init(kernel::interrupt::syscall_entry as *const () as u64);
    kernel::time::register_timer_ticks_provider(
        arch::x86_64::interrupt_descriptor_table::get_ticks,
    );
    kernel::task::architecture::register_context_switch(arch::x86_64::switch_context);
    kernel::task::architecture::register_user_mode_entry(arch::x86_64::enter_user_mode);
    kernel::task::architecture::register_returnable_user_mode_entry(
        arch::x86_64::enter_user_mode_once,
    );
    kernel::task::user_mode::register_selectors(kernel::task::user_mode::UserModeSelectors {
        data: arch::x86_64::global_descriptor_table::USER_DATA_SELECTOR,
        code: arch::x86_64::global_descriptor_table::USER_CODE_SELECTOR,
    });
    crate::serial_println!("[ok   ] Architecture initialized.");
    let user_selectors = kernel::task::user_mode::get_selectors();
    crate::serial_println!(
        "[task ] Ring 3 selectors installed. code={:#06x}, data={:#06x}",
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

    crate::serial_println!("[driver] Initializing mouse...");
    kernel::driver::input::mouse::init();
    crate::serial_println!("[ok   ] Mouse initialized.");

    arch::x86_64::enable_interrupts();
}

fn verify_kernel_filesystem() {
    let _ = kernel::filesystem::write(
        kernel::filesystem::STANDARD_OUTPUT,
        b"[fs   ] Standard output is connected to /dev/console.\n",
    );
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_ERROR, b"");

    kernel::filesystem::mount_ram_file("/hello.txt", b"hello from ramfs\n");
    let descriptor =
        kernel::filesystem::open("/hello.txt").expect("ramfs smoke test file must open");
    let mut buffer = [0_u8; 32];
    let bytes_read =
        kernel::filesystem::read(descriptor, &mut buffer).expect("ramfs smoke test must read");
    kernel::filesystem::close(descriptor).expect("ramfs smoke test descriptor must close");
    let _ = kernel::filesystem::write(kernel::filesystem::STANDARD_OUTPUT, &buffer[..bytes_read]);

    let null_descriptor =
        kernel::filesystem::open("/dev/null").expect("null device must open during smoke test");
    let _ = kernel::filesystem::write(null_descriptor, b"discarded");
    kernel::filesystem::close(null_descriptor).expect("null descriptor must close");

    let _ = kernel::filesystem::read(kernel::filesystem::STANDARD_INPUT, &mut buffer);
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
    crate::serial_println!("[serial] ExitBootServices OK.");
    let mut frame_allocator = kernel::memory::frame_allocator::BumpFrameAllocator::new();
    add_conventional_memory_regions(&mut frame_allocator, mmap.entries());

    // ────────────────────────────────────────────────
    // Kernel Phase (UEFI Services unavailable)
    // ────────────────────────────────────────────────
    crate::serial_println!("[info ] ManaOS Kernel phase started.");

    let framebuffer_size = get_framebuffer_size(framebuffer_info);
    let backbuffer_ptr = allocate_backbuffer(&mut frame_allocator, framebuffer_size);

    kernel::boot::initialize(
        &mut frame_allocator,
        mmap.entries(),
        framebuffer_info,
        kernel::driver::display::font::FontAssets {
            inter: font_inter,
            noto: font_noto,
        },
        backbuffer_ptr,
    );
    kernel::filesystem::initialize();
    crate::serial_println!("[fs   ] Kernel filesystem initialized.");
    verify_kernel_filesystem();
    initialize_scheduler();
    initialize_architecture_and_drivers();

    crate::serial_println!("[ok   ] ManaOS Kernel is alive.");

    // Calibrate TSC for profiling before user tasks can preempt the bootstrap task.
    kernel::profiler::calibrate_tsc();

    kernel::runtime::initialize();

    let user_stack_top = kernel::memory::user_stack::allocate_user_stack(&mut frame_allocator, 4);
    let user_entry_point =
        kernel::memory::user_stack::allocate_user_write_demo(&mut frame_allocator);
    let user_task_id = kernel::task::spawn_user_task(user_entry_point, user_stack_top);
    crate::serial_println!("[ok   ] User task spawned.");
    crate::serial_println!("[ok   ] User demo started.");
    if let Some(exit_code) = kernel::task::run_user_task_once(user_task_id) {
        crate::serial_println!("[ok   ] UI resumed after user exit: code={}", exit_code);
    }

    // Main Loop
    loop {
        kernel::runtime::tick();

        // For maximum performance testing, we don't hlt.
        // x86_64::instructions::hlt();
    }
}
