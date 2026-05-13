//! # kernel::driver::input::mouse
//!
//! ## Owns
//! - PS/2 mouse byte queue from interrupt handler to main loop
//! - Mouse packet processing and state updates
//!
//! ## Does NOT own
//! - Cursor drawing primitives (-> kernel::driver::display::framebuffer)
//! - Interrupt routing (-> arch::x86_64::interrupt_descriptor_table)
//!
//! ## Public API
//! - [`init`] - Initialize PS/2 mouse hardware
//! - [`push_byte`] - Called from interrupt handler only
//! - [`process_packets`] - Called from main loop only
//! - [`draw_cursor`] - Draw the cursor from the current mouse state

mod hardware;
mod packet;
mod state;

use crate::kernel::driver::input::mouse::packet::MousePacket;
use crate::kernel::driver::input::mouse::state::process_packet;
use crate::kernel::sync::ring_buffer::LockFreeRingBuffer;

pub use state::draw_cursor;

static MOUSE_QUEUE: LockFreeRingBuffer<u8, 1024> = LockFreeRingBuffer::new();

pub fn push_byte(byte: u8) {
    let _ = MOUSE_QUEUE.push(byte);
}

pub fn init() {
    crate::kernel::driver::input::mouse::hardware::init();
}

pub fn process_packets() {
    let mut b0 = 0;
    let mut b1 = 0;
    let mut count = 0;

    while let Some(byte) = MOUSE_QUEUE.pop() {
        match count {
            0 => {
                if (byte & 0x08) != 0 {
                    b0 = byte;
                    count = 1;
                }
            }
            1 => {
                b1 = byte;
                count = 2;
            }
            2 => {
                if let Some(packet) = MousePacket::parse(b0, b1, byte) {
                    process_packet(packet);
                }
                count = 0;
            }
            _ => count = 0,
        }
    }
}
