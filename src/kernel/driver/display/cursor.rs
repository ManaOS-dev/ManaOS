//! Display-owned cursor rendering.

use crate::kernel::driver::display::color::Color;
use core::sync::atomic::{AtomicBool, Ordering};

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Draw the cursor from the current input mouse state.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
pub fn draw_cursor() {
    let state = crate::kernel::driver::input::mouse::get_state();
    let _ = crate::kernel::driver::display::framebuffer::try_with_graphics_mut(|graphics| {
        let width = i32::try_from(graphics.info.horizontal_resolution).unwrap_or(0);
        let height = i32::try_from(graphics.info.vertical_resolution).unwrap_or(0);

        let x = state.x.clamp(0, width - 16);
        let y = state.y.clamp(0, height - 16);

        if INITIALIZED.load(Ordering::Acquire) {
            let (old_x, old_y) = graphics.last_cursor_pos;
            graphics.restore_cursor_area();
            graphics.flush_rect(old_x, old_y, 16, 16);
        }

        let cursor_x = usize::try_from(x).unwrap_or(0);
        let cursor_y = usize::try_from(y).unwrap_or(0);

        graphics.save_cursor_area(cursor_x, cursor_y);
        graphics.draw_filled_rectangle(cursor_x, cursor_y, 5, 5, Color::rgb(0xFF, 0, 0));
        graphics.flush_rect(cursor_x, cursor_y, 16, 16);
        INITIALIZED.store(true, Ordering::Release);
    });
}
