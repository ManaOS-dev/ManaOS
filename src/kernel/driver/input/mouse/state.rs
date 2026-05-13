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

pub fn process_packet(packet: MousePacket) {
    let mut state = STATE.lock();
    state.left = packet.left_button;
    state.right = packet.right_button;
    state.middle = packet.middle_button;
    state.x += packet.delta_x;
    state.y += packet.delta_y;
}

pub fn draw_cursor() {
    let state = STATE.lock();
    let _ = crate::kernel::driver::display::framebuffer::try_with_graphics_mut(|graphics| {
        let width = graphics.info.horizontal_resolution as i32;
        let height = graphics.info.vertical_resolution as i32;

        let mut x = state.x;
        let mut y = state.y;

        if x < 0 {
            x = 0;
        }
        if y < 0 {
            y = 0;
        }
        if x > width - 16 {
            x = width - 16;
        }
        if y > height - 16 {
            y = height - 16;
        }

        if INITIALIZED.load(Ordering::Acquire) {
            let (old_x, old_y) = graphics.last_cursor_pos;
            graphics.restore_cursor_area();
            graphics.flush_rect(old_x, old_y, 16, 16);
        }

        graphics.save_cursor_area(x as usize, y as usize);
        graphics.draw_filled_rectangle(x as usize, y as usize, 5, 5, 0xff0000);
        graphics.flush_rect(x as usize, y as usize, 16, 16);
        INITIALIZED.store(true, Ordering::Release);
    });
}
