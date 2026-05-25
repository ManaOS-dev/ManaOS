//! Userland command name dispatch.

use super::{cat, clear, echo, grep, CommandError, CommandOutput};

pub(super) fn run_stage(
    command: &str,
    input: &[u8],
    output: &mut CommandOutput,
) -> Result<(), CommandError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(CommandError::EmptyCommand);
    }

    let (name, arguments) = command
        .split_once(' ')
        .map_or((command, ""), |(name, arguments)| (name, arguments.trim()));
    match name {
        "cat" => cat::run(arguments, input, output),
        "clear" => clear::run(arguments, input, output),
        "echo" => echo::run(arguments, input, output),
        "grep" => grep::run(arguments, input, output),
        _ => Err(CommandError::UnknownCommand),
    }
}
