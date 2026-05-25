//! `echo` kernel console command.

use super::output::{CommandEffect, CommandError, CommandOutput};

#[allow(clippy::unnecessary_wraps)]
pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    Ok(CommandEffect::Output(CommandOutput::single(
        alloc::string::ToString::to_string(arguments),
    )))
}
