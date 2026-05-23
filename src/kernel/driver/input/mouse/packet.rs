/// A decoded PS/2 mouse packet.
pub struct MousePacket {
    /// Horizontal movement delta.
    pub delta_x: i32,
    /// Vertical movement delta.
    pub delta_y: i32,
    /// Whether the left button is pressed.
    pub left_button: bool,
    /// Whether the right button is pressed.
    pub right_button: bool,
    /// Whether the middle button is pressed.
    pub middle_button: bool,
}

impl MousePacket {
    /// Parse three raw PS/2 bytes into a `MousePacket`.
    ///
    /// Returns `None` if the sync bit is missing.
    pub fn parse(b0: u8, b1: u8, b2: u8) -> Option<Self> {
        if b0 & 0x08 == 0 {
            return None;
        }
        Some(Self {
            delta_x: i32::from(b1.cast_signed()),
            delta_y: -i32::from(b2.cast_signed()),
            left_button: b0 & 0x01 != 0,
            right_button: b0 & 0x02 != 0,
            middle_button: b0 & 0x04 != 0,
        })
    }
}
