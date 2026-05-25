//! `stat` kernel console command.

use super::context;
use super::output::{CommandEffect, CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if arguments.is_empty() {
        return Err(CommandError::MissingArgument("stat"));
    }

    let path = context::resolve_path(arguments);
    let metadata = crate::kernel::filesystem::metadata(&path)
        .map_err(|_| CommandError::StatFailed(path.clone()))?;
    Ok(CommandEffect::Output(CommandOutput::single(
        alloc::format!(
            "{}: type={} size={} writable={}",
            path,
            context::file_type_label(metadata.file_type),
            metadata.size,
            metadata.writable
        ),
    )))
}
