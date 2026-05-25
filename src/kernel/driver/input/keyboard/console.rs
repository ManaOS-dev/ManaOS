//! Keyboard-to-console input dispatch.

use pc_keyboard::KeyCode;

/// Process a decoded Unicode keyboard character.
pub(super) fn process_character(character: char) {
    if character == '`' {
        crate::kernel::console::toggle();
        return;
    }

    if !crate::kernel::console::is_open() {
        return;
    }

    match character {
        '\n' | '\r' => crate::kernel::console::submit(),
        '\u{8}' | '\u{7f}' => crate::kernel::console::push_backspace(),
        _ => crate::kernel::console::push_character(character),
    }
}

/// Process a decoded non-Unicode keyboard key code.
pub(super) fn process_key_code(key_code: KeyCode) {
    if key_code == KeyCode::Escape {
        crate::kernel::console::toggle();
        return;
    }

    if !crate::kernel::console::is_open() {
        return;
    }

    match key_code {
        KeyCode::Return | KeyCode::NumpadEnter => crate::kernel::console::submit(),
        KeyCode::Backspace => crate::kernel::console::push_backspace(),
        _ => crate::log_debug!("keyboard", "raw key: {:?}", key_code),
    }
}
