//! # `kernel::driver::display::command`
//!
//! ## Owns
//! - Drawing command queue (asynchronous rendering)
//!
//! ## Public API
//! - [`DrawCommand`] enum for supported operations
//! - [`push_command`] to add to queue
//! - [`process_commands`] to render from queue

use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::display::framebuffer::Font;
use crate::kernel::sync::ring_buffer::LockFreeRingBuffer;
use alloc::string::String;

/// Supported drawing commands.
#[derive(Debug, Clone)]
pub enum DrawCommand {
    /// Fill rectangle: (x, y, width, height, color)
    FillRect(usize, usize, usize, usize, Color),
    /// Draw line: (x1, y1, x2, y2, color)
    #[allow(dead_code)]
    Line(i32, i32, i32, i32, Color),
    /// Flush specific area: (x, y, width, height)
    FlushRect(usize, usize, usize, usize),
    /// Draw text: (font, x, y, scale, color, text)
    Text(Font, usize, usize, f32, Color, String),
}

static COMMAND_QUEUE: LockFreeRingBuffer<DrawCommand, 2048> = LockFreeRingBuffer::new();

/// Push a drawing command to the queue.
pub fn push_command(cmd: DrawCommand) {
    let _ = COMMAND_QUEUE.push(cmd);
}

/// Process all pending drawing commands.
pub fn process_commands() {
    use crate::kernel::driver::display::framebuffer;

    while let Some(cmd) = COMMAND_QUEUE.pop() {
        framebuffer::try_with_graphics_mut(|graphics| match cmd {
            DrawCommand::FillRect(x, y, width, height, color) => {
                graphics.draw_filled_rectangle(x, y, width, height, color);
            }
            DrawCommand::Line(x1, y1, x2, y2, color) => {
                graphics.draw_line(x1, y1, x2, y2, color);
            }
            DrawCommand::FlushRect(x, y, width, height) => {
                graphics.flush_rect(x, y, width, height);
            }
            DrawCommand::Text(font, x, y, scale, color, text) => {
                graphics.draw_text(font, x, y, scale, color, &text);
            }
        });
    }
}
