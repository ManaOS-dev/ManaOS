//! `cat` userland command.

use super::{CommandError, CommandOutput};
use crate::io::FileDescriptor;

pub(super) fn run(
    arguments: &str,
    _input: &[u8],
    output: &mut CommandOutput,
) -> Result<(), CommandError> {
    if arguments.is_empty() {
        return Err(CommandError::MissingArgument);
    }

    let file = FileDescriptor::open_read_only(arguments).map_err(|_| CommandError::FileError)?;
    let mut buffer = [0_u8; 128];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|_| CommandError::FileError)?;
        if bytes_read == 0 {
            break;
        }
        if let Err(error) = output.write(&buffer[..bytes_read]) {
            let _ = file.close();
            return Err(error);
        }
        if bytes_read < buffer.len() {
            break;
        }
    }
    file.close().map_err(|_| CommandError::FileError)?;
    Ok(())
}
