//! `clear` userland command.

use super::{CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    _input: &[u8],
    _output: &mut CommandOutput,
) -> Result<(), CommandError> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(CommandError::UnknownCommand)
    }
}
