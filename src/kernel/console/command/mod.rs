//! # `kernel::console::command`
//!
//! ## Owns
//! - Kernel console command parsing
//! - Dispatch to command-focused handlers
//!
//! ## Does NOT own
//! - Console rendering and input state (-> `kernel::console`)
//! - Filesystem namespace state (-> `kernel::filesystem`)
//! - Storage controller probing (-> `kernel::driver::storage`)
//!
//! ## Public API
//! - [`execute`] - Parse and run one submitted console command

mod filesystem;
mod system;

use alloc::format;
use alloc::string::{String, ToString};

pub(super) fn execute(command: &str) {
    if command.is_empty() {
        return;
    }

    push_output(format!("> {command}"));
    let (name, argument) = split_command(command);
    match name {
        "help" => push_output(
            "commands: help clear pwd cd ls stat mounts hexdump cat read echo syscalls".to_string(),
        ),
        "clear" if argument.is_empty() => clear_output(),
        "pwd" if argument.is_empty() => filesystem::push_working_directory(),
        "cd" => filesystem::change_directory(argument),
        "ls" => filesystem::list_directory(argument),
        "stat" => filesystem::push_stat_output(argument),
        "mounts" if argument.is_empty() => filesystem::push_mounts_output(),
        "hexdump" => filesystem::push_hexdump_output(argument),
        "ticks" if argument.is_empty() => system::push_ticks_output(),
        "fps" if argument.is_empty() => system::push_fps_output(),
        "storage" | "partitions" if argument.is_empty() => system::push_storage_output(),
        "cat" | "read" => filesystem::push_file_output(name, argument),
        "echo" => push_output(argument.to_string()),
        "syscalls" if argument.is_empty() => system::push_syscalls_output(),
        _ => push_output(format!("unknown command: {command}")),
    }
}

fn split_command(command: &str) -> (&str, &str) {
    command
        .split_once(' ')
        .map_or((command, ""), |(name, argument)| (name, argument.trim()))
}

fn push_output(line: String) {
    super::push_output(line);
}

fn clear_output() {
    super::clear_output();
}
