pub struct MousePacket {
    pub delta_x: i32,
    pub delta_y: i32,
    pub left_button: bool,
    pub right_button: bool,
    pub middle_button: bool,
}

impl MousePacket {
    /// Parse three raw PS/2 bytes into a MousePacket.
    pub fn parse(b0: u8, b1: u8, b2: u8) -> Option<Self> {
        if b0 & 0x08 == 0 {
            return None;
        }
        Some(Self {
            delta_x: b1 as i8 as i32,
            delta_y: -(b2 as i8 as i32),
            left_button: b0 & 0x01 != 0,
            right_button: b0 & 0x02 != 0,
            middle_button: b0 & 0x04 != 0,
        })
    }
}
