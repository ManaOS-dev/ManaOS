//! Keyboard scancode decoding.

use pc_keyboard::{layouts, DecodedKey, HandleControl, PS2Keyboard, ScancodeSet1};
use spin::Mutex;

use crate::kernel::driver::input::keyboard::{console, queue};

const MAX_SCANCODES_PER_TICK: usize = 16;

static KEYBOARD: Mutex<PS2Keyboard<layouts::Us104Key, ScancodeSet1>> =
    Mutex::new(PS2Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    ));

/// Decode and process pending scancodes.
pub(super) fn process_input() {
    let mut keyboard = KEYBOARD.lock();
    let mut processed_scancodes = 0;

    while processed_scancodes < MAX_SCANCODES_PER_TICK {
        let Some(scancode) = queue::pop_scancode() else {
            break;
        };
        processed_scancodes += 1;

        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(decoded_key) = keyboard.process_keyevent(key_event) {
                match decoded_key {
                    DecodedKey::Unicode(character) => {
                        console::process_character(character);
                    }
                    DecodedKey::RawKey(key_code) => {
                        console::process_key_code(key_code);
                    }
                }
            }
        }
    }
}
