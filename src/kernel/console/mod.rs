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
//! - [`render_if_dirty`] - Redraw the command console when state changed

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::{LazyLock, Mutex};

use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::display::command::{push_command, DrawCommand};
use crate::kernel::driver::display::font::Font;

const MAX_INPUT_BYTES: usize = 96;
const MAX_OUTPUT_LINES: usize = 6;
const CONSOLE_HEIGHT: usize = 150;
const CONSOLE_PADDING: usize = 12;
const LINE_HEIGHT: usize = 18;
const TITLE_HEIGHT: usize = 24;

mod command;

static STATE: LazyLock<Mutex<ConsoleState>> = LazyLock::new(|| Mutex::new(ConsoleState::new()));

struct ConsoleState {
    input: String,
    output: Vec<String>,
    dirty: bool,
    open: bool,
    needs_clear: bool,
}

impl ConsoleState {
    fn new() -> Self {
        Self {
            input: String::new(),
            output: Vec::new(),
            dirty: false,
            open: false,
            needs_clear: false,
        }
    }
}

/// Return whether the command console is open.
pub fn is_open() -> bool {
    STATE.lock().open
}

/// Toggle command console visibility.
pub fn toggle() {
    let mut state = STATE.lock();
    state.open = !state.open;
    state.dirty = true;
    state.needs_clear = true;
}

/// Push one decoded keyboard character into the command input buffer.
pub fn push_character(character: char) {
    if character.is_control() {
        return;
    }

    let mut state = STATE.lock();
    if state.open && state.input.len() < MAX_INPUT_BYTES {
        state.input.push(character);
        state.dirty = true;
    }
}

/// Delete one pending input character.
pub fn push_backspace() {
    let mut state = STATE.lock();
    if state.open && state.input.pop().is_some() {
        state.dirty = true;
    }
}

/// Execute the current input line.
pub fn submit() {
    let command = {
        let mut state = STATE.lock();
        if !state.open {
            return;
        }
        let command = state.input.trim().to_string();
        state.input.clear();
        state.dirty = true;
        command
    };

    command::execute(&command);
}

/// Redraw the command console overlay when state changed.
pub fn render_if_dirty() {
    let (input, output, open, needs_clear) = {
        let mut state = STATE.lock();
        if !state.dirty {
            return;
        }
        state.dirty = false;
        let needs_clear = state.needs_clear;
        state.needs_clear = false;
        (
            state.input.clone(),
            state.output.clone(),
            state.open,
            needs_clear,
        )
    };

    let screen_width = crate::kernel::driver::display::framebuffer::with_graphics(|g| {
        g.info.horizontal_resolution
    });
    let screen_height =
        crate::kernel::driver::display::framebuffer::with_graphics(|g| g.info.vertical_resolution);
    let console_y = screen_height.saturating_sub(CONSOLE_HEIGHT);
    let console_width = screen_width;

    if needs_clear {
        push_command(DrawCommand::FillRect(
            0,
            console_y,
            console_width,
            CONSOLE_HEIGHT,
            Color::rgb(0x00, 0x00, 0x05),
        ));
        push_command(DrawCommand::FlushRect(
            0,
            console_y,
            console_width,
            CONSOLE_HEIGHT,
        ));
    }

    if !open {
        return;
    }

    push_command(DrawCommand::FillRect(
        0,
        console_y,
        console_width,
        CONSOLE_HEIGHT,
        Color::rgb(0x08, 0x0A, 0x0E),
    ));
    push_command(DrawCommand::FillRect(
        0,
        console_y,
        console_width,
        2,
        Color::rgb(0x34, 0x94, 0xDB),
    ));
    push_command(DrawCommand::Text(
        Font::Inter,
        CONSOLE_PADDING,
        console_y + 6,
        14.0,
        Color::rgb(0xA7, 0xC7, 0xE7),
        "ManaOS Command".to_string(),
    ));

    let mut text_y = console_y + TITLE_HEIGHT + CONSOLE_PADDING;
    for line in output.iter().rev().take(MAX_OUTPUT_LINES).rev() {
        push_command(DrawCommand::Text(
            Font::Inter,
            CONSOLE_PADDING,
            text_y,
            15.0,
            Color::rgb(0xD8, 0xE2, 0xF0),
            line.clone(),
        ));
        text_y += LINE_HEIGHT;
    }

    let prompt = format!("mana> {input}");
    push_command(DrawCommand::Text(
        Font::Inter,
        CONSOLE_PADDING,
        console_y + CONSOLE_HEIGHT - 30,
        16.0,
        Color::rgb(0x7C, 0xF2, 0xA0),
        prompt,
    ));
    push_command(DrawCommand::FlushRect(
        0,
        console_y,
        console_width,
        CONSOLE_HEIGHT,
    ));
}

fn clear_output() {
    let mut state = STATE.lock();
    state.output.clear();
    state.dirty = true;
}

fn push_output(line: String) {
    let mut state = STATE.lock();
    if state.output.len() == MAX_OUTPUT_LINES {
        state.output.remove(0);
    }
    state.output.push(line);
    state.dirty = true;
}
