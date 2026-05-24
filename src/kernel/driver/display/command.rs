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
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

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

static COMMAND_QUEUE: Mutex<CommandQueue<2048>> = Mutex::new(CommandQueue::new());
static DROPPED_COMMANDS: AtomicU64 = AtomicU64::new(0);

struct CommandQueue<const N: usize> {
    buffer: [Option<DrawCommand>; N],
    head: usize,
    tail: usize,
}

impl<const N: usize> CommandQueue<N> {
    const fn new() -> Self {
        Self {
            buffer: [const { None }; N],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, command: DrawCommand) -> Result<(), DrawCommand> {
        let next_head = (self.head + 1) % N;
        if next_head == self.tail {
            return Err(command);
        }

        self.buffer[self.head] = Some(command);
        self.head = next_head;
        Ok(())
    }

    fn pop(&mut self) -> Option<DrawCommand> {
        if self.head == self.tail {
            return None;
        }

        let command = self.buffer[self.tail].take();
        self.tail = (self.tail + 1) % N;
        command
    }
}

/// Push a drawing command to the queue.
pub fn push_command(command: DrawCommand) {
    if COMMAND_QUEUE.lock().push(command).is_err() {
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
        while let Some(command) = COMMAND_QUEUE.lock().pop() {
            process_command(graphics, command);
        }
    });
}

fn process_command(
    graphics: &mut crate::kernel::driver::display::framebuffer::GraphicsDriver,
    command: DrawCommand,
) {
    match command {
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
