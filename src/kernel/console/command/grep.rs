//! `grep` kernel console command.

use super::output::{CommandEffect, CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if arguments.is_empty() {
        return Err(CommandError::MissingArgument("grep"));
    }

    let mut output = CommandOutput::new();
    for line in input {
        if line.contains(arguments) {
            output.push(line.clone());
        }
    }
    Ok(CommandEffect::Output(output))
}
