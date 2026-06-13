//! `help` kernel console command.

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
        "commands: help clear pwd cd ls stat mounts memory hexdump cat read grep echo syscalls tasks"
            .to_string(),
    )))
}
