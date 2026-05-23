//! # `kernel::driver::display::command`
//!
//! ## Owns
//! - Drawing command queue (asynchronous rendering)
//!
//! ## Public API
//! - [`DrawCommand`] enum for supported operations
//! - [`push_command`] to add to queue
//! - [`process_commands`] to render from queue
//! - [`get_dropped_command_count`] to inspect queue pressure

use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::display::font::Font;
use crate::kernel::sync::ring_buffer::LockFreeRingBuffer;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};

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
static DROPPED_COMMANDS: AtomicU64 = AtomicU64::new(0);

/// Push a drawing command to the queue.
pub fn push_command(cmd: DrawCommand) {
    if COMMAND_QUEUE.push(cmd).is_err() {
        DROPPED_COMMANDS.fetch_add(1, Ordering::Relaxed);
    }
}

/// Return the number of drawing commands dropped because the queue was full.
#[allow(dead_code)]
pub fn get_dropped_command_count() -> u64 {
    DROPPED_COMMANDS.load(Ordering::Relaxed)
}

/// Process all pending drawing commands.
pub fn process_commands() {
    use crate::kernel::driver::display::framebuffer;

    let _ = framebuffer::try_with_graphics_mut(|graphics| {
        while let Some(cmd) = COMMAND_QUEUE.pop() {
            process_command(graphics, cmd);
        }
    });
}

fn process_command(
    graphics: &mut crate::kernel::driver::display::framebuffer::GraphicsDriver,
    cmd: DrawCommand,
) {
    match cmd {
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
    }
}
