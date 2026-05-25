//! `grep` userland command.

use super::{CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    input: &[u8],
    output: &mut CommandOutput,
) -> Result<(), CommandError> {
    if arguments.is_empty() {
        return Err(CommandError::MissingArgument);
    }

    let needle = arguments.as_bytes();
    for line in input.split_inclusive(|byte| *byte == b'\n') {
        if contains(line, needle) {
            output.write(line)?;
        }
    }

    Ok(())
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }

    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
