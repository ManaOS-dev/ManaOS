//! `ls` kernel console command.

use super::context;
use super::output::{CommandEffect, CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    let path = if arguments.is_empty() {
        context::get_current_directory()
    } else {
        context::resolve_path(arguments)
    };
    let entries = crate::kernel::filesystem::list_directory(&path)
        .map_err(|_| CommandError::DirectoryListFailed(path.clone()))?;

    let mut output = CommandOutput::new();
    if entries.is_empty() {
        output.push(alloc::format!("{path}: empty"));
        return Ok(CommandEffect::Output(output));
    }

    for entry in entries.iter().take(6) {
        output.push(alloc::format!(
            "{} {} {}",
            context::file_type_label(entry.metadata.file_type),
            entry.metadata.size,
            entry.name
        ));
    }
    Ok(CommandEffect::Output(output))
}
