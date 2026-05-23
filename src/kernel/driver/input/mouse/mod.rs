//! # `kernel::driver::input::mouse`
//!
//! ## Owns
//! - PS/2 mouse byte queue from interrupt handler to main loop
//! - Mouse packet processing and state updates
//!
//! ## Does NOT own
//! - Cursor drawing primitives (-> `kernel::driver::display::framebuffer`)
//! - Interrupt routing (-> `arch::x86_64::interrupt_descriptor_table`)
//!
//! ## Public API
//! - [`init`] - Initialize PS/2 mouse hardware
//! - [`push_byte`] - Called from interrupt handler only
//! - [`process_packets`] - Called from main loop only
//! - [`get_state`] - Read current mouse state

mod hardware;
mod packet;
mod state;

use crate::kernel::driver::input::mouse::packet::MousePacket;
use crate::kernel::driver::input::mouse::state::process_packet;
use crate::kernel::sync::ring_buffer::LockFreeRingBuffer;
use spin::Mutex;

#[allow(unused_imports)]
pub use state::{get_state, MouseState};

static MOUSE_QUEUE: LockFreeRingBuffer<u8, 1024> = LockFreeRingBuffer::new();
static PACKET_DECODER: Mutex<PacketDecoder> = Mutex::new(PacketDecoder::new());

/// Push one raw PS/2 mouse byte from the interrupt path.
pub fn push_byte(byte: u8) {
    let _ = MOUSE_QUEUE.push(byte);
}

/// Initialize PS/2 mouse hardware.
pub fn init() {
    crate::kernel::driver::input::mouse::hardware::init();
}

/// Process queued mouse bytes into packets and update mouse state.
pub fn process_packets() {
    let mut decoder = PACKET_DECODER.lock();

    while let Some(byte) = MOUSE_QUEUE.pop() {
        if let Some(packet) = decoder.push_byte(byte) {
            process_packet(&packet);
        }
    }
}

struct PacketDecoder {
    first_byte: u8,
    second_byte: u8,
    count: u8,
}

impl PacketDecoder {
    const fn new() -> Self {
        Self {
            first_byte: 0,
            second_byte: 0,
            count: 0,
        }
    }

    fn push_byte(&mut self, byte: u8) -> Option<MousePacket> {
        match self.count {
            0 => {
                if (byte & 0x08) != 0 {
                    self.first_byte = byte;
                    self.count = 1;
                }
                None
            }
            1 => {
                self.second_byte = byte;
                self.count = 2;
                None
            }
            2 => {
                self.count = 0;
                MousePacket::parse(self.first_byte, self.second_byte, byte)
            }
            _ => {
                self.count = 0;
                None
            }
        }
    }
}
