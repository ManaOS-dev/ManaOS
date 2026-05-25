//! Single-pipe kernel console command execution.

use super::dispatch;
use super::output::{CommandEffect, CommandError};

pub(super) fn run_line(command: &str) -> Result<CommandEffect, CommandError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(CommandError::EmptyCommand);
    }

    if let Some((left, right)) = command.split_once('|') {
        if right.contains('|') {
            return Err(CommandError::TooManyPipes);
        }

        let left_output = dispatch::run_stage(left.trim(), &[])?;
        let CommandEffect::Output(left_output) = left_output else {
            return Err(CommandError::NotPipeable("clear"));
        };
        return dispatch::run_stage(right.trim(), left_output.lines());
    }

    dispatch::run_stage(command, &[])
}
