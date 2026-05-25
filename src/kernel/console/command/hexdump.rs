//! `hexdump` kernel console command.

use super::context;
use super::output::{CommandEffect, CommandError, CommandOutput};

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if arguments.is_empty() {
        return Err(CommandError::MissingArgument("hexdump"));
    }

    let path = context::resolve_path(arguments);
    let file_descriptor = crate::kernel::filesystem::open(&path)
        .map_err(|_| CommandError::FileOpenFailed(path.clone()))?;
    let _ = crate::kernel::filesystem::seek(file_descriptor, 0);
    let mut buffer = [0_u8; 16];
    let result = crate::kernel::filesystem::read(file_descriptor, &mut buffer);
    let _ = crate::kernel::filesystem::close(file_descriptor);
    let bytes_read = result.map_err(|_| CommandError::FileReadFailed(path))?;

    Ok(CommandEffect::Output(CommandOutput::single(
        alloc::format!("0000: {}", context::HexBytes(&buffer[..bytes_read])),
    )))
}
