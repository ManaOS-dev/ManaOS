//! `syscalls` kernel console command.

use super::output::{CommandEffect, CommandError, CommandOutput};
use alloc::string::ToString;

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if !arguments.is_empty() {
        return Err(CommandError::UnknownCommand);
    }

    Ok(CommandEffect::Output(CommandOutput::single(
        "syscalls: read write open close fstat lseek mmap munmap brk getdents64 exit exit_group openat getpid"
            .to_string(),
    )))
}
