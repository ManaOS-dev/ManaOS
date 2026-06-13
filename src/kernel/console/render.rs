//! Kernel console overlay rendering.

use alloc::format;
use alloc::string::{String, ToString};

use crate::kernel::driver::display::color::Color;
use crate::kernel::driver::display::command::{push_command, DrawCommand};
use crate::kernel::driver::display::font::Font;

const CONSOLE_HEIGHT: usize = 210;
const CONSOLE_PADDING: usize = 12;
const LINE_HEIGHT: usize = 18;
const TITLE_HEIGHT: usize = 64;

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
    push_command(DrawCommand::FillRect(
        CONSOLE_PADDING,
        console_y + 30,
        console_width.saturating_sub(CONSOLE_PADDING * 2),
        1,
        Color::rgb(0x1D, 0x2B, 0x3A),
    ));
    push_status_lines(console_y);

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

fn push_status_lines(console_y: usize) {
    push_command(DrawCommand::Text(
        Font::Inter,
        CONSOLE_PADDING,
        console_y + 32,
        12.0,
        Color::rgb(0xA8, 0xF0, 0xC6),
        scheduler_status_line(),
    ));
    push_command(DrawCommand::Text(
        Font::Inter,
        CONSOLE_PADDING,
        console_y + 46,
        12.0,
        Color::rgb(0xF2, 0xD5, 0x7C),
        memory_reclaim_status_line(),
    ));
}

pub(super) fn verify_status_strip_smoke() -> bool {
    let scheduler_line = scheduler_status_line();
    let memory_line = memory_reclaim_status_line();
    scheduler_line.contains("tasks total=")
        && scheduler_line.contains("user=")
        && scheduler_line.contains("active_spaces=")
        && scheduler_line.contains("preempt=")
        && scheduler_line.contains("resume=")
        && memory_line.contains("memory kernel_stacks_reclaimed=")
        && memory_line.contains("writable_pages=")
        && memory_line.contains("virtual_pages=")
}

fn input_cursor_marker(input: &str, cursor: usize) -> String {
    let cursor = cursor.min(input.len());
    let mut output = String::new();
    output.push_str(&input[..cursor]);
    output.push('_');
    output.push_str(&input[cursor..]);
    output
}

fn scheduler_status_line() -> String {
    let Some(diagnostics) = crate::kernel::task::get_scheduler_diagnostics() else {
        return "tasks unavailable".to_string();
    };
    let states = diagnostics.states();
    format!(
        "tasks total={} user={} active_spaces={} states R{} Run{} B{} F{} preempt={} resume={}",
        diagnostics.total_tasks(),
        diagnostics.user_tasks(),
        diagnostics.active_user_address_spaces(),
        states.ready(),
        states.running(),
        states.blocked(),
        states.finished(),
        diagnostics.timer_preemptions(),
        diagnostics.user_resumes()
    )
}

fn memory_reclaim_status_line() -> String {
    let Some(diagnostics) = crate::kernel::task::get_scheduler_diagnostics() else {
        return "memory unavailable".to_string();
    };
    format!(
        "memory kernel_stacks_reclaimed={} writable_pages={} virtual_pages={}",
        diagnostics.reclaimed_user_kernel_stacks(),
        diagnostics.reclaimed_user_kernel_stack_writable_pages(),
        diagnostics.reclaimed_user_kernel_stack_virtual_pages()
    )
}
