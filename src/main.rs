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

use alloc::format;
use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::display::framebuffer::Font;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;

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

    let buffer = unsafe { core::slice::from_raw_parts_mut(ptr, size) };
    file.read(buffer).expect("Failed to read file");

    buffer
}

#[entry]
fn main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    // ────────────────────────────────────────────────
    // Boot Phase (UEFI Services available)
    // ────────────────────────────────────────────────
    kernel::logger::init(&mut st);

    log::info!("ManaOS booting (HAL edition)...");

    // Acquire UEFI graphics output protocol.
    let framebuffer_info = {
        let graphics_output_handle = st
            .boot_services()
            .get_handle_for_protocol::<GraphicsOutput>()
            .expect("GraphicsOutput handle is required for ManaOS framebuffer setup");
        let mut graphics_output = st
            .boot_services()
            .open_protocol_exclusive::<GraphicsOutput>(graphics_output_handle)
            .expect("GraphicsOutput protocol is required for ManaOS framebuffer setup");
        kernel::driver::display::framebuffer::get_info(&mut graphics_output)
    };

    // Get Memory Map and save CONVENTIONAL regions
    let mmap_buf = &mut [0u8; 4096 * 4];
    let mmap = st
        .boot_services()
        .memory_map(mmap_buf)
        .expect("Failed to get memory map");

    let mut frame_allocator = kernel::memory::frame_allocator::BumpFrameAllocator::new();

    for desc in mmap.entries() {
        if desc.ty == uefi::table::boot::MemoryType::CONVENTIONAL {
            frame_allocator.add_region(desc.phys_start, desc.page_count);
        }
    }

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

    // Initialize Paging (Identity Mapping)
    let framebuffer_base = framebuffer_info.base_ptr as u64;
    let framebuffer_size =
        (framebuffer_info.stride * framebuffer_info.vertical_resolution * 4) as u64;
    // SAFETY: The frame allocator owns conventional memory from the boot memory map,
    // and the framebuffer range comes from the active UEFI graphics mode.
    unsafe {
        kernel::memory::paging::init(
            &mut frame_allocator,
            mmap.entries(),
            framebuffer_base,
            framebuffer_size,
        );
    }
    serial_println!("[paging] Page table switched.");

    // Allocate Backbuffer (same size as framebuffer)
    let backbuffer_pages = framebuffer_size.div_ceil(4096);
    let backbuffer_physical_address = frame_allocator
        .allocate_frames(backbuffer_pages)
        .expect("OOM: failed to allocate framebuffer backbuffer");
    let backbuffer_ptr = backbuffer_physical_address as *mut u8;

    // Initialize Kernel Heap
    let heap_start_raw = frame_allocator
        .allocate_frames(kernel::memory::heap::HEAP_PAGES)
        .expect("OOM: failed to allocate pages for kernel heap");
    let heap_start = usize::try_from(heap_start_raw).expect("Failed to convert heap address");

    // SAFETY: heap_start was allocated from the frame allocator and is exclusively
    // reserved for the kernel heap.
    unsafe {
        kernel::memory::heap::init(heap_start);
    }
    crate::serial_println!(
        "[heap ] Initialized at {:#010x}, size: {} MB",
        heap_start,
        kernel::memory::heap::HEAP_SIZE / (1024 * 1024)
    );

    // ────────────────────────────────────────────────
    // Kernel Phase (UEFI Services unavailable)
    // ────────────────────────────────────────────────
    crate::serial_println!("[info ] ManaOS Kernel phase started.");

    kernel::task::initialize();
    kernel::task::spawn(idle_task);
    let task_id = kernel::task::get_current_task_id()
        .expect("scheduler must expose a bootstrap task after initialization");
    crate::serial_println!("[ok   ] Scheduler initialized. current task: {}", task_id);

    // Initialize Architecture (descriptor tables, interrupt controller, interrupts)
    arch::init();
    crate::serial_println!("[ok   ] Architecture initialized.");

    // Initialize Drivers

    // Initialize Graphics with Double Buffering
    kernel::driver::display::framebuffer::init_global_graphics(
        framebuffer_info,
        kernel::driver::display::framebuffer::FontAssets {
            inter: font_inter,
            noto: font_noto,
        },
        backbuffer_ptr,
    );

    {
        kernel::driver::display::framebuffer::with_graphics(|graphics| {
            graphics.clear_gradient();

            // Draw Sample UI using primitives
            graphics.draw_filled_rectangle(50, 50, 400, 250, Color::rgb(0x11, 0x11, 0x11));
            graphics.draw_rectangle(50, 50, 400, 250, Color::rgb(0x44, 0x44, 0x44));
            graphics.draw_line(50, 80, 450, 80, Color::rgb(0x44, 0x44, 0x44));

            graphics.draw_text(Font::Inter, 70, 60, 20.0, Color::WHITE, "ManaOS");

            graphics.draw_text(Font::Inter, 100, 180, 32.0, Color::rgb(0x00, 0xAA, 0xFF), "graphics !!");

            graphics.draw_text(Font::NotoSansJP, 100, 300, 20.0, Color::WHITE, "日本語");

            // Final flush to show initial screen
            graphics.flush();
        });
    }

    crate::serial_println!("[ok   ] ManaOS Kernel is alive.");

    // Calibrate TSC for profiling
    kernel::profiler::calibrate_tsc();

    let mut frame_count = 0;
    let mut last_fps_ticks = arch::x86_64::interrupt_descriptor_table::get_ticks();
    let mut fps = 0;

    // Main Loop
    loop {
        kernel::driver::input::keyboard::process_input();
        kernel::driver::input::mouse::process_packets();
        kernel::driver::input::mouse::draw_cursor();

        frame_count += 1;

        let current_ticks = arch::x86_64::interrupt_descriptor_table::get_ticks();

        // Update FPS every 500ms
        if current_ticks - last_fps_ticks >= 500 {
            fps = frame_count * 1000 / (current_ticks - last_fps_ticks);
            frame_count = 0;
            last_fps_ticks = current_ticks;
        }

        // Draw HUD (FPS counter) to backbuffer
        let _ = kernel::driver::display::framebuffer::try_with_graphics_mut(|graphics| {
            // Clear a small area for FPS
            graphics.draw_filled_rectangle(
                graphics.info.horizontal_resolution - 150,
                10,
                140,
                30,
                Color::BLACK,
            );

            let fps_text = format!("FPS: {fps}");
            graphics.draw_text(
                Font::Inter,
                graphics.info.horizontal_resolution - 140,
                15,
                16.0,
                Color::rgb(0x00, 0xFF, 0x00),
                &fps_text,
            );

            graphics.flush_rect(graphics.info.horizontal_resolution - 150, 10, 140, 30);
        });

        // For maximum performance testing, we don't hlt.
        // x86_64::instructions::hlt();
    }
}
