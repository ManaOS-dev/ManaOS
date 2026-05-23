use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::input::mouse::packet::MousePacket;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

static STATE: Mutex<MouseState> = Mutex::new(MouseState {
    x: 0,
    y: 0,
    left: false,
    right: false,
    middle: false,
});
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn process_packet(packet: &MousePacket) {
    let mut state = STATE.lock();
    state.left = packet.left_button;
    state.right = packet.right_button;
    state.middle = packet.middle_button;
    state.x += packet.delta_x;
    state.y += packet.delta_y;
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
pub fn draw_cursor() {
    let state = STATE.lock();
    let _ = crate::kernel::driver::display::framebuffer::try_with_graphics_mut(|graphics| {
        let width = i32::try_from(graphics.info.horizontal_resolution).unwrap_or(0);
        let height = i32::try_from(graphics.info.vertical_resolution).unwrap_or(0);

        let mut x = state.x;
        let mut y = state.y;

        x = x.clamp(0, width - 16);
        y = y.clamp(0, height - 16);

        if INITIALIZED.load(Ordering::Acquire) {
            let (old_x, old_y) = graphics.last_cursor_pos;
            graphics.restore_cursor_area();
            graphics.flush_rect(old_x, old_y, 16, 16);
        }

        let ux = usize::try_from(x).unwrap_or(0);
        let uy = usize::try_from(y).unwrap_or(0);

        graphics.save_cursor_area(ux, uy);
        graphics.draw_filled_rectangle(ux, uy, 5, 5, Color::rgb(0xFF, 0, 0));
        graphics.flush_rect(ux, uy, 16, 16);
        INITIALIZED.store(true, Ordering::Release);
    });
}
