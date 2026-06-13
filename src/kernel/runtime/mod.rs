//! # `kernel::runtime`
//!
//! ## Owns
//! - Main loop execution and tick processing
//!
//! ## Public API
//! - [`tick`] - Run one iteration of the main loop

use crate::kernel;
use crate::kernel::driver::display::color::Color;
use core::sync::atomic::{AtomicU64, Ordering};

static FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static LAST_FPS_TICKS: AtomicU64 = AtomicU64::new(0);
static FPS: AtomicU64 = AtomicU64::new(0);

/// Initialize runtime state.
pub fn initialize() {
    let ticks = kernel::time::get_timer_ticks();
    LAST_FPS_TICKS.store(ticks, Ordering::Relaxed);
}

/// Return the last calculated frames-per-second value.
pub fn get_fps() -> u64 {
    FPS.load(Ordering::Relaxed)
}

/// Run one iteration of the main loop.
pub fn tick() {
    kernel::driver::input::keyboard::process_input();
    kernel::driver::input::mouse::process_packets();
    kernel::console::render_if_dirty();

    FRAME_COUNT.fetch_add(1, Ordering::Relaxed);

    let current_ticks = kernel::time::get_timer_ticks();
    let last_ticks = LAST_FPS_TICKS.load(Ordering::Relaxed);

    let elapsed_ticks = current_ticks.saturating_sub(last_ticks);

    // Update FPS and UI every 500ms
    if elapsed_ticks >= 500 {
        let frame_count = FRAME_COUNT.swap(0, Ordering::Relaxed);
        let fps = frame_count
            .saturating_mul(crate::shared::TIMER_TICKS_PER_SECOND)
            .checked_div(elapsed_ticks)
            .unwrap_or(0);
        FPS.store(fps, Ordering::Relaxed);
        LAST_FPS_TICKS.store(current_ticks, Ordering::Relaxed);

        // Queue FPS HUD draw commands.
        let screen_width =
            kernel::driver::display::framebuffer::with_graphics(|g| g.info.horizontal_resolution);
        let hud_width = 140usize.min(screen_width);
        let hud_x = screen_width.saturating_sub(150);
        let text_x = screen_width.saturating_sub(140);
        kernel::driver::display::command::push_command(
            kernel::driver::display::command::DrawCommand::FillRect(
                hud_x,
                10,
                hud_width,
                30,
                Color::BLACK,
            ),
        );

        let fps_text = alloc::format!("FPS: {fps}");
        kernel::driver::display::command::push_command(
            kernel::driver::display::command::DrawCommand::Text(
                kernel::driver::display::font::Font::Inter,
                text_x,
                15,
                16.0,
                Color::rgb(0x00, 0xFF, 0x00),
                fps_text,
            ),
        );

        kernel::driver::display::command::push_command(
            kernel::driver::display::command::DrawCommand::FlushRect(hud_x, 10, hud_width, 30),
        );
    }

    kernel::driver::display::command::process_commands();

    let mouse_state = kernel::driver::input::mouse::get_state();
    kernel::driver::display::cursor::draw_cursor(mouse_state.x, mouse_state.y);
}
