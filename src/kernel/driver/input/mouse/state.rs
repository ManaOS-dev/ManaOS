use crate::kernel::driver::input::mouse::packet::MousePacket;
use spin::Mutex;

/// Current PS/2 mouse position and button state.
#[derive(Debug, Clone, Copy)]
pub struct MouseState {
    /// Horizontal cursor position in pixels.
    pub x: i32,
    /// Vertical cursor position in pixels.
    pub y: i32,
    /// Whether the left button is pressed.
    pub left: bool,
    /// Whether the right button is pressed.
    pub right: bool,
    /// Whether the middle button is pressed.
    pub middle: bool,
}

static STATE: Mutex<MouseState> = Mutex::new(MouseState {
    x: 0,
    y: 0,
    left: false,
    right: false,
    middle: false,
});

/// Apply one decoded mouse packet to the current mouse state.
pub fn process_packet(packet: &MousePacket) {
    let mut state = STATE.lock();
    state.left = packet.left_button;
    state.right = packet.right_button;
    state.middle = packet.middle_button;
    state.x += packet.delta_x;
    state.y += packet.delta_y;

    crate::kernel::driver::display::framebuffer::with_graphics(|graphics| {
        let width = i32::try_from(graphics.info.horizontal_resolution).unwrap_or(0);
        let height = i32::try_from(graphics.info.vertical_resolution).unwrap_or(0);

        state.x = state.x.clamp(0, (width - 16).max(0));
        state.y = state.y.clamp(0, (height - 16).max(0));
    });
}

/// Return a snapshot of the current mouse state.
pub fn get_state() -> MouseState {
    *STATE.lock()
}
