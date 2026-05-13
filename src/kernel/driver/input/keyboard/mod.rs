//! # Keyboard Driver
//!
//! Handles PS/2 keyboard scan code decoding.
//!
//! ## Responsibilities
//! - Buffering raw scancodes (interrupts)
//! - Translating scancodes to key events (main loop)
//!
//! ## NOT Responsibilities
//! - Handling mouse interrupts (→ input::mouse)

use crate::kernel::sync::ring_buffer::LockFreeRingBuffer;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use spin::Mutex;

static SCANCODE_QUEUE: Mutex<LockFreeRingBuffer<u8, 128>> = Mutex::new(LockFreeRingBuffer::new());
static KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(Keyboard::new(
    ScancodeSet1::new(),
    layouts::Us104Key,
    HandleControl::Ignore,
));

/// Push a raw scancode from the keyboard interrupt.
pub fn push_scancode(scancode: u8) {
    if let Some(queue) = SCANCODE_QUEUE.try_lock() {
        let _ = queue.push(scancode);
    }
}

/// Decode and process pending scancodes.
pub fn process_input() {
    let mut keyboard = KEYBOARD.lock();
    if let Some(queue) = SCANCODE_QUEUE.try_lock() {
        let mut processed = 0;
        loop {
            if processed >= 16 {
                break;
            }
            if let Some(scancode) = queue.pop() {
                processed += 1;
                if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                    if let Some(key) = keyboard.process_keyevent(key_event) {
                        match key {
                            DecodedKey::Unicode(character) => {
                                crate::serial_print!("{}", character);
                                crate::serial_println!(" [kb] key: {}", character);
                            }
                            DecodedKey::RawKey(key) => {
                                crate::serial_println!(" [kb] raw: {:?}", key)
                            }
                        }
                    }
                }
            } else {
                break;
            }
        }
    }
}
