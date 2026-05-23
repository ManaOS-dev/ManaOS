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
use uefi::table::boot::MemoryDescriptor;

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
fn load_file(st: &SystemTable<Boot>, path: &str) -> &'static mut [u8] {
    use uefi::proto::media::file::FileInfo;
    use uefi::table::boot::MemoryType;

    let fs_handle = st
        .boot_services()
        .get_handle_for_protocol::<SimpleFileSystem>()
        .expect("Failed to get SimpleFileSystem handle");
    let mut fs = st
        .boot_services()
        .open_protocol_exclusive::<SimpleFileSystem>(fs_handle)
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
    let ptr = st
        .boot_services()
        .allocate_pool(MemoryType::LOADER_DATA, size)
        .expect("Failed to allocate pool for file");

    // SAFETY: allocate_pool returned a valid pointer to a LOADER_DATA buffer of
    // exactly size bytes, and the buffer remains owned by the boot phase.
    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr, size) };
    file.read(buffer).expect("Failed to read file");

    buffer
}

fn get_framebuffer_info(
    st: &SystemTable<Boot>,
) -> kernel::driver::display::framebuffer::FrameBufferInfo {
    let graphics_output_handle = st
        .boot_services()
        .get_handle_for_protocol::<GraphicsOutput>()
        .expect("GraphicsOutput handle is required for ManaOS framebuffer setup");
    let mut graphics_output = st
        .boot_services()
        .open_protocol_exclusive::<GraphicsOutput>(graphics_output_handle)
        .expect("GraphicsOutput protocol is required for ManaOS framebuffer setup");
    kernel::driver::display::framebuffer::get_info(&mut graphics_output)
}

fn add_conventional_memory_regions<'a>(
    frame_allocator: &mut kernel::memory::frame_allocator::BumpFrameAllocator,
    memory_descriptors: impl Iterator<Item = &'a MemoryDescriptor>,
) {
    for descriptor in memory_descriptors {
        if descriptor.ty == uefi::table::boot::MemoryType::CONVENTIONAL {
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
    arch::init();
    crate::serial_println!("[ok   ] Architecture initialized.");

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

#[entry]
fn main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    let _ = image;

    // ────────────────────────────────────────────────
    // Boot Phase (UEFI Services available)
    // ────────────────────────────────────────────────
    kernel::logger::init(&mut st);

    log::info!("ManaOS booting (HAL edition)...");

    let framebuffer_info = get_framebuffer_info(&st);

    // Get Memory Map and save CONVENTIONAL regions
    let mmap_buf = &mut [0u8; 4096 * 4];
    let mmap = st
        .boot_services()
        .memory_map(mmap_buf)
        .expect("Failed to get memory map");

    let mut frame_allocator = kernel::memory::frame_allocator::BumpFrameAllocator::new();
    add_conventional_memory_regions(&mut frame_allocator, mmap.entries());

    log::info!("Calling ExitBootServices...");

    // Pre-load fonts before exiting boot services
    let font_inter = load_file(&st, "Inter.ttf");
    let font_noto = load_file(&st, "NotoSansJP.ttf");

    // ────────────────────────────────────────────────
    // ExitBootServices
    // ────────────────────────────────────────────────
    kernel::logger::disable();
    let (_st_runtime, mmap) = st.exit_boot_services();

    // ────────────────────────────────────────────────
    // Kernel Phase
    // ────────────────────────────────────────────────
    kernel::serial::init();
    crate::serial_println!("[serial] ExitBootServices OK.");

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
        kernel::driver::display::framebuffer::FontAssets {
            inter: font_inter,
            noto: font_noto,
        },
        backbuffer_ptr,
    );
    initialize_scheduler();
    initialize_architecture_and_drivers();

    crate::serial_println!("[ok   ] ManaOS Kernel is alive.");

    // Calibrate TSC for profiling
    kernel::profiler::calibrate_tsc();

    kernel::runtime::initialize();

    // Main Loop
    loop {
        kernel::runtime::tick();

        // For maximum performance testing, we don't hlt.
        // x86_64::instructions::hlt();
    }
}
