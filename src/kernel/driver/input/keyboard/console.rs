//! Keyboard-to-console input dispatch.

use pc_keyboard::KeyCode;

/// Process a decoded Unicode keyboard character.
pub(super) fn process_character(character: char) {
    if character == '`' {
        crate::kernel::console::toggle();
        return;
    }

    if !crate::kernel::console::is_open() {
        super::stdin::push_character(character);
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
        match key_code {
            KeyCode::Return | KeyCode::NumpadEnter => super::stdin::push_byte(b'\n'),
            KeyCode::Backspace => super::stdin::push_byte(0x08),
            _ => {}
        }
        return;
    }

    match key_code {
        KeyCode::Return | KeyCode::NumpadEnter => crate::kernel::console::submit(),
        KeyCode::Backspace => crate::kernel::console::push_backspace(),
        KeyCode::ArrowLeft => crate::kernel::console::move_cursor_left(),
        KeyCode::ArrowRight => crate::kernel::console::move_cursor_right(),
        KeyCode::ArrowUp => crate::kernel::console::load_previous_history(),
        KeyCode::ArrowDown => crate::kernel::console::load_next_history(),
        KeyCode::Home => crate::kernel::console::move_cursor_home(),
        KeyCode::End => crate::kernel::console::move_cursor_end(),
        KeyCode::PageUp => crate::kernel::console::scroll_up(),
        KeyCode::PageDown => crate::kernel::console::scroll_down(),
        _ => crate::log_debug!("keyboard", "raw key: {:?}", key_code),
    }
}
