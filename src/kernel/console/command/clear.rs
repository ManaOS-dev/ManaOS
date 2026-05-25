//! `clear` kernel console command.

use super::output::{CommandEffect, CommandError};

pub(super) fn run(
    arguments: &str,
    input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if !arguments.is_empty() || !input.is_empty() {
        return Err(CommandError::NotPipeable("clear"));
    }

    Ok(CommandEffect::Clear)
}
