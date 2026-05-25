//! `cat` and `read` kernel console commands.

use super::context;
use super::output::{CommandEffect, CommandError, CommandOutput};

const READ_BUFFER_BYTES: usize = 512;

pub(super) fn run(
    command_name: &'static str,
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if arguments.is_empty() {
        return Err(CommandError::MissingArgument(command_name));
    }

    let path = context::resolve_path(arguments);
    let file_descriptor = crate::kernel::filesystem::open(&path)
        .map_err(|_| CommandError::FileOpenFailed(path.clone()))?;
    let mut buffer = [0_u8; READ_BUFFER_BYTES];
    let result = crate::kernel::filesystem::read(file_descriptor, &mut buffer);
    let _ = crate::kernel::filesystem::close(file_descriptor);
    let bytes_read = result.map_err(|_| CommandError::FileReadFailed(path))?;

    let mut output = CommandOutput::new();
    context::push_text_lines(&buffer[..bytes_read], &mut output);
    Ok(CommandEffect::Output(output))
}
