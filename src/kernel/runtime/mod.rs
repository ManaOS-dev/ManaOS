//! # `kernel::runtime`
//!
//! ## Owns
//! - Main loop execution and tick processing
//!
//! ## Public API
//! - [`tick`] - Run one iteration of the main loop

use crate::arch;
use crate::kernel;
use crate::kernel::driver::display::color::Color;
use core::sync::atomic::{AtomicU64, Ordering};

static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_FPS_TICKS: AtomicU64 = AtomicU64::new(0);
static FPS: AtomicU64 = AtomicU64::new(0);

/// Initialize runtime state.
pub fn initialize() {
    let ticks = arch::x86_64::interrupt_descriptor_table::get_ticks();
    LAST_FPS_TICKS.store(ticks, Ordering::Relaxed);
}

/// Run one iteration of the main loop.
pub fn tick() {
    kernel::driver::input::keyboard::process_input();
    kernel::driver::input::mouse::process_packets();
    kernel::driver::input::mouse::draw_cursor();

    FRAME_COUNT.fetch_add(1, Ordering::Relaxed);

    let current_ticks = arch::x86_64::interrupt_descriptor_table::get_ticks();
    let last_ticks = LAST_FPS_TICKS.load(Ordering::Relaxed);

    // Update FPS and UI every 500ms
    if current_ticks - last_ticks >= 500 {
        let frame_count = FRAME_COUNT.swap(0, Ordering::Relaxed);
        let fps = frame_count * 1000 / (current_ticks - last_ticks);
        FPS.store(fps, Ordering::Relaxed);
        LAST_FPS_TICKS.store(current_ticks, Ordering::Relaxed);

        // Queue FPS HUD draw commands
        let res_w =
            kernel::driver::display::framebuffer::with_graphics(|g| g.info.horizontal_resolution);
        kernel::driver::display::command::push_command(
            kernel::driver::display::command::DrawCommand::FillRect(
                res_w - 150,
                10,
                140,
                30,
                Color::BLACK,
            ),
        );

        let fps_text = alloc::format!("FPS: {fps}");
        kernel::driver::display::command::push_command(
            kernel::driver::display::command::DrawCommand::Text(
                kernel::driver::display::framebuffer::Font::Inter,
                res_w - 140,
                15,
                16.0,
                Color::rgb(0x00, 0xFF, 0x00),
                fps_text,
            ),
        );

        kernel::driver::display::command::push_command(
            kernel::driver::display::command::DrawCommand::FlushRect(res_w - 150, 10, 140, 30),
        );
    }

    kernel::driver::display::command::process_commands();
}
