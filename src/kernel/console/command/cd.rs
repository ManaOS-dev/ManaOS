//! `cd` kernel console command.

use super::context;
use super::output::{CommandEffect, CommandError, CommandOutput};
use alloc::string::String;

pub(super) fn run(arguments: &str, input: &[String]) -> Result<CommandEffect, CommandError> {
    if !input.is_empty() {
        return Err(CommandError::NotPipeable("cd"));
    }

    let path = if arguments.is_empty() {
        String::from("/")
    } else {
        context::resolve_path(arguments)
    };
    match crate::kernel::filesystem::metadata(&path) {
        Ok(metadata) if metadata.file_type == crate::kernel::filesystem::FileType::Directory => {
            context::set_current_directory(path);
            Ok(CommandEffect::Output(CommandOutput::new()))
        }
        Ok(_) => Ok(CommandEffect::Output(CommandOutput::single(
            alloc::format!("cd: not a directory: {path}"),
        ))),
        Err(_) => Ok(CommandEffect::Output(CommandOutput::single(
            alloc::format!("cd: no such directory: {path}"),
        ))),
    }
}
