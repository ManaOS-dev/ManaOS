//! # `kernel::console`
//!
//! ## Owns
//! - Kernel console module composition
//! - Public console service re-exports
//!
//! ## Does NOT own
//! - Keyboard scancode decoding (-> `kernel::driver::input::keyboard`)
//! - Raw display drawing primitives (-> `kernel::driver::display`)
//! - Console input state and rendering facade logic (-> `service`)
//!
//! ## Public API
//! - [`is_open`] - Return whether the command console is open
//! - [`toggle`] - Toggle command console visibility
//! - [`push_character`] - Push one decoded keyboard character
//! - [`push_backspace`] - Delete one pending input character
//! - [`submit`] - Execute the current input line
//! - [`verify_pipeline_smoke`] - Run a non-interactive pipeline smoke check
//! - [`verify_command_smoke`] - Run a non-interactive command smoke check
//! - [`verify_command_smoke_contains`] - Run a non-interactive command content smoke check
//! - [`verify_status_strip_smoke`] - Probe the console overlay status strip
//! - [`render_if_dirty`] - Redraw the command console when state changed

mod command;
mod render;
mod service;
mod state;

pub(in crate::kernel::console) use service::{clear_output, push_output};
pub use service::{
    is_open, load_next_history, load_previous_history, move_cursor_end, move_cursor_home,
    move_cursor_left, move_cursor_right, push_backspace, push_character, render_if_dirty,
    scroll_down, scroll_up, submit, toggle, verify_command_smoke, verify_command_smoke_contains,
    verify_pipeline_smoke, verify_status_strip_smoke,
};
