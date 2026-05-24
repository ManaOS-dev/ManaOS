//! Display-owned cursor rendering.

use crate::kernel::driver::display::color::Color;
use core::sync::atomic::{AtomicBool, Ordering};

/// Width and height of the software cursor in pixels.
pub(super) const CURSOR_SIZE: usize = 16;
const CURSOR_PIXELS: [&[u8; CURSOR_SIZE]; CURSOR_SIZE] = [
    b"O...............",
    b"OW..............",
    b"OWW.............",
    b"OWWW............",
    b"OWWWW...........",
    b"OWWWWW..........",
    b"OWWWWWW.........",
    b"OWWWWWWW........",
    b"OWWWWWWWW.......",
    b"OWWWWWWWWO......",
    b"OWWWOOO.........",
    b"OWO.OWO.........",
    b"OO..OWO.........",
    b"....OWO.........",
    b".....OWO........",
    b".....OO.........",
];

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Draw the cursor at the provided screen position.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
pub fn draw_cursor(x: i32, y: i32) {
    let _ = crate::kernel::driver::display::framebuffer::try_with_graphics_mut(|graphics| {
        let width = i32::try_from(graphics.info.horizontal_resolution).unwrap_or(0);
        let height = i32::try_from(graphics.info.vertical_resolution).unwrap_or(0);

        let x = x.clamp(0, (width - i32::try_from(CURSOR_SIZE).unwrap_or(0)).max(0));
        let y = y.clamp(0, (height - i32::try_from(CURSOR_SIZE).unwrap_or(0)).max(0));
        let cursor_x = usize::try_from(x).unwrap_or(0);
        let cursor_y = usize::try_from(y).unwrap_or(0);

        if INITIALIZED.load(Ordering::Acquire) {
            let (old_x, old_y) = graphics.last_cursor_pos;
            if (old_x, old_y) != (cursor_x, cursor_y) {
                graphics.flush_rect(old_x, old_y, CURSOR_SIZE, CURSOR_SIZE);
            }
        }

        graphics.save_cursor_area(cursor_x, cursor_y);
        draw_pointer_shape(graphics, cursor_x, cursor_y);
        graphics.flush_rect(cursor_x, cursor_y, CURSOR_SIZE, CURSOR_SIZE);
        graphics.restore_cursor_area();
        INITIALIZED.store(true, Ordering::Release);
    });
}

fn draw_pointer_shape(
    graphics: &crate::kernel::driver::display::framebuffer::GraphicsDriver,
    x: usize,
    y: usize,
) {
    let outline = Color::BLACK.to_u32();
    let fill = Color::WHITE.to_u32();

    for (pixel_y, row) in CURSOR_PIXELS.iter().enumerate() {
        for (pixel_x, pixel) in row.iter().enumerate() {
            match pixel {
                b'O' => graphics.put_pixel(x + pixel_x, y + pixel_y, outline),
                b'W' => graphics.put_pixel(x + pixel_x, y + pixel_y, fill),
                _ => {}
            }
        }
    }
}
