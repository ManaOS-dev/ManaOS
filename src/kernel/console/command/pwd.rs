//! `pwd` kernel console command.

use super::context;
use super::output::{CommandEffect, CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if !arguments.is_empty() {
        return Err(CommandError::UnknownCommand);
    }

    Ok(CommandEffect::Output(CommandOutput::single(
        context::get_current_directory(),
    )))
}
