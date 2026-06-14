//! Console input, command, and render facade functions.

use super::{command, render, state};
use alloc::string::String;

/// Return whether the command console is open.
pub fn is_open() -> bool {
    state::is_open()
}

/// Toggle command console visibility.
pub fn toggle() {
    state::toggle();
}

/// Push one decoded keyboard character into the command input buffer.
pub fn push_character(character: char) {
    state::push_character(character);
}

/// Delete one pending input character.
pub fn push_backspace() {
    state::push_backspace();
}

/// Move the input cursor left by one character.
pub fn move_cursor_left() {
    state::move_cursor_left();
}

/// Move the input cursor right by one character.
pub fn move_cursor_right() {
    state::move_cursor_right();
}

/// Move the input cursor to the start of the line.
pub fn move_cursor_home() {
    state::move_cursor_home();
}

/// Move the input cursor to the end of the line.
pub fn move_cursor_end() {
    state::move_cursor_end();
}

/// Load the previous command from history.
pub fn load_previous_history() {
    state::load_previous_history();
}

/// Load the next command from history.
pub fn load_next_history() {
    state::load_next_history();
}

/// Scroll console output up.
pub fn scroll_up() {
    state::scroll_up();
}

/// Scroll console output down.
pub fn scroll_down() {
    state::scroll_down();
}

/// Execute the current input line.
pub fn submit() {
    let Some(command) = state::take_submitted_command() else {
        return;
    };

    command::execute(&command);
}

/// Run a non-interactive pipeline command and return the number of output lines.
pub fn verify_pipeline_smoke(command: &str) -> Option<usize> {
    command::verify_pipeline_smoke(command)
}

/// Run a non-interactive command and return the number of output lines.
pub fn verify_command_smoke(command: &str) -> Option<usize> {
    command::verify_command_smoke(command)
}

/// Run a non-interactive command and require output text fragments.
pub fn verify_command_smoke_contains(command: &str, required_needles: &[&str]) -> Option<usize> {
    command::verify_command_smoke_contains(command, required_needles)
}

/// Verify that the console overlay can format its scheduler status strip.
pub fn verify_status_strip_smoke() -> bool {
    render::verify_status_strip_smoke()
}

/// Redraw the command console overlay when state changed.
pub fn render_if_dirty() {
    render::render_if_dirty();
}

pub(in crate::kernel::console) fn clear_output() {
    state::clear_output();
}

pub(in crate::kernel::console) fn push_output(line: String) {
    state::push_output(line);
}
