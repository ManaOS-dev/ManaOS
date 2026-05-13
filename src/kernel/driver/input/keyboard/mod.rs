//! # `kernel::driver::input::keyboard`
//!
//! ## Owns
//! - PS/2 keyboard scancode queue
//! - Keyboard state decoding
//!
//! ## Does NOT own
//! - Interrupt routing (-> `arch::x86_64::interrupt_descriptor_table`)
//!
//! ## Public API
//! - [`push_scancode`] - Called from interrupt handler
//! - [`process_input`] - Called from main loop

use crate::kernel::sync::ring_buffer::LockFreeRingBuffer;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use spin::Mutex;

static SCANCODE_QUEUE: LockFreeRingBuffer<u8, 128> = LockFreeRingBuffer::new();
static KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(Keyboard::new(
    ScancodeSet1::new(),
    layouts::Us104Key,
    HandleControl::Ignore,
));

/// Push a raw scancode from the keyboard interrupt.
pub fn push_scancode(scancode: u8) {
    let _ = SCANCODE_QUEUE.push(scancode);
}

/// Decode and process pending scancodes.
pub fn process_input() {
    let mut keyboard = KEYBOARD.lock();
    let mut processed = 0;
    while processed < 16 {
        if let Some(scancode) = SCANCODE_QUEUE.pop() {
            processed += 1;
            if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                if let Some(key) = keyboard.process_keyevent(key_event) {
                    match key {
                        DecodedKey::Unicode(character) => {
                            crate::serial_print!("{}", character);
                        }
                        DecodedKey::RawKey(key) => {
                            crate::serial_println!(" [kb] raw: {:?}", key);
                        }
                    }
                }
            }
        } else {
            break;
        }
    }
}
