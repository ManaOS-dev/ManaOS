//! Kernel console overlay rendering.

use alloc::format;
use alloc::string::{String, ToString};

use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::display::command::{push_command, DrawCommand};
use crate::kernel::driver::display::font::Font;

const CONSOLE_HEIGHT: usize = 150;
const CONSOLE_PADDING: usize = 12;
const LINE_HEIGHT: usize = 18;
const TITLE_HEIGHT: usize = 24;

pub(super) fn render_if_dirty() {
    let Some(snapshot) = super::state::take_render_snapshot() else {
        return;
    };

    let screen_width = crate::kernel::driver::display::framebuffer::with_graphics(|g| {
        g.info.horizontal_resolution
    });
    let screen_height =
        crate::kernel::driver::display::framebuffer::with_graphics(|g| g.info.vertical_resolution);
    let console_y = screen_height.saturating_sub(CONSOLE_HEIGHT);
    let console_width = screen_width;

    if snapshot.needs_clear {
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

    if !snapshot.open {
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
    for line in &snapshot.output {
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

    let cursor_marker = input_cursor_marker(&snapshot.input, snapshot.cursor);
    let prompt = format!("mana> {cursor_marker}");
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

fn input_cursor_marker(input: &str, cursor: usize) -> String {
    let cursor = cursor.min(input.len());
    let mut output = String::new();
    output.push_str(&input[..cursor]);
    output.push('_');
    output.push_str(&input[cursor..]);
    output
}
