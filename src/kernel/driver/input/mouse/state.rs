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
}

/// Return a snapshot of the current mouse state.
pub fn get_state() -> MouseState {
    *STATE.lock()
}
