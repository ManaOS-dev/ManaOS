//! Display scene rendering helpers.

use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::display::framebuffer::{self, Font};

/// Draw the initial boot screen after the framebuffer driver is initialized.
pub fn draw_boot_screen() {
    framebuffer::with_graphics(|graphics| {
        graphics.clear_gradient();

        graphics.draw_filled_rectangle(50, 50, 400, 250, Color::rgb(0x11, 0x11, 0x11));
        graphics.draw_rectangle(50, 50, 400, 250, Color::rgb(0x44, 0x44, 0x44));
        graphics.draw_line(50, 80, 450, 80, Color::rgb(0x44, 0x44, 0x44));

        graphics.draw_text(Font::Inter, 70, 60, 20.0, Color::WHITE, "ManaOS");
        graphics.draw_text(
            Font::Inter,
            100,
            180,
            32.0,
            Color::rgb(0x00, 0xAA, 0xFF),
            "graphics !!",
        );
        graphics.draw_text(Font::NotoSansJP, 100, 300, 20.0, Color::WHITE, "日本語");

        graphics.flush();
    });
}
