//! Single-pipe command line execution.

use super::dispatch::run_stage;
use super::{CommandError, CommandOutput};

/// Execute one command line with optional `left | right` pipeline.
pub fn run_line(line: &str, output: &mut CommandOutput) -> Result<(), CommandError> {
    output.clear();
    let line = line.trim();
    if line.is_empty() {
        return Err(CommandError::EmptyCommand);
    }

    if let Some((left, right)) = line.split_once('|') {
        if right.contains('|') {
            return Err(CommandError::TooManyPipes);
        }

        let mut intermediate = CommandOutput::new();
        run_stage(left.trim(), &[], &mut intermediate)?;
        run_stage(right.trim(), intermediate.as_bytes(), output)?;
        return Ok(());
    }

    run_stage(line, &[], output)
}
