//! Kernel console command name dispatch.

use super::output::{CommandEffect, CommandError};
use super::{
    cat, cd, clear, echo, fps, grep, help, hexdump, ls, memory, mounts, pwd, stat, storage,
    syscalls, tasks, ticks,
};
use alloc::string::String;

pub(super) fn run_stage(command: &str, input: &[String]) -> Result<CommandEffect, CommandError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(CommandError::EmptyCommand);
    }

    let (name, arguments) = command
        .split_once(' ')
        .map_or((command, ""), |(name, arguments)| (name, arguments.trim()));
    match name {
        "help" => help::run(arguments, input),
        "clear" => clear::run(arguments, input),
        "pwd" => pwd::run(arguments, input),
        "cd" => cd::run(arguments, input),
        "ls" => ls::run(arguments, input),
        "stat" => stat::run(arguments, input),
        "mounts" => mounts::run(arguments, input),
        "memory" => memory::run(arguments, input),
        "hexdump" => hexdump::run(arguments, input),
        "ticks" => ticks::run(arguments, input),
        "fps" => fps::run(arguments, input),
        "storage" | "partitions" => storage::run(arguments, input),
        "cat" => cat::run("cat", arguments, input),
        "read" => cat::run("read", arguments, input),
        "echo" => echo::run(arguments, input),
        "grep" => grep::run(arguments, input),
        "syscalls" => syscalls::run(arguments, input),
        "tasks" => tasks::run(arguments, input),
        _ => Err(CommandError::UnknownCommand),
    }
}
