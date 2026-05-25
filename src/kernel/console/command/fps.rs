//! `fps` kernel console command.

use super::output::{CommandEffect, CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if !arguments.is_empty() {
        return Err(CommandError::UnknownCommand);
    }

    Ok(CommandEffect::Output(CommandOutput::single(
        alloc::format!("fps={}", crate::kernel::runtime::get_fps()),
    )))
}
