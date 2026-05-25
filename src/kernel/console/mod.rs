//! # `kernel::console`
//!
//! ## Owns
//! - Kernel command input buffer
//! - Kernel command dispatch
//! - Command console overlay rendering
//!
//! ## Does NOT own
//! - Keyboard scancode decoding (-> `kernel::driver::input::keyboard`)
//! - Raw display drawing primitives (-> `kernel::driver::display`)
//!
//! ## Public API
//! - [`is_open`] - Return whether the command console is open
//! - [`toggle`] - Toggle command console visibility
//! - [`push_character`] - Push one decoded keyboard character
//! - [`push_backspace`] - Delete one pending input character
//! - [`submit`] - Execute the current input line
//! - [`verify_pipeline_smoke`] - Run a non-interactive pipeline smoke check
//! - [`render_if_dirty`] - Redraw the command console when state changed

mod command;
mod render;
mod state;

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

/// Redraw the command console overlay when state changed.
pub fn render_if_dirty() {
    render::render_if_dirty();
}

fn clear_output() {
    state::clear_output();
}

fn push_output(line: String) {
    state::push_output(line);
}
