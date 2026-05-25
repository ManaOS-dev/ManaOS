//! `echo` userland command.

use super::{CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    _input: &[u8],
    output: &mut CommandOutput,
) -> Result<(), CommandError> {
    output.write(arguments.as_bytes())?;
    output.write(b"\n")
}
