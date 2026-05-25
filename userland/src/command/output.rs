//! Fixed-buffer command output types.

/// Maximum bytes carried between userland pipeline stages.
pub const COMMAND_BUFFER_BYTES: usize = 512;

/// Userland command execution error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandError {
    /// The command name is empty.
    EmptyCommand,
    /// The command name is not registered.
    UnknownCommand,
    /// A required command argument is missing.
    MissingArgument,
    /// The command line contains more than one pipe.
    TooManyPipes,
    /// The output did not fit in the fixed command buffer.
    OutputTooLarge,
    /// A file operation failed.
    FileError,
}

/// Fixed-size command output buffer.
pub struct CommandOutput {
    bytes: [u8; COMMAND_BUFFER_BYTES],
    length: usize,
}

impl CommandOutput {
    /// Create an empty output buffer.
    pub const fn new() -> Self {
        Self {
            bytes: [0; COMMAND_BUFFER_BYTES],
            length: 0,
        }
    }

    /// Return the written output bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }

    /// Remove all output bytes.
    pub fn clear(&mut self) {
        self.length = 0;
    }

    /// Append bytes to this output buffer.
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), CommandError> {
        let end = self
            .length
            .checked_add(bytes.len())
            .ok_or(CommandError::OutputTooLarge)?;
        if end > self.bytes.len() {
            return Err(CommandError::OutputTooLarge);
        }

        self.bytes[self.length..end].copy_from_slice(bytes);
        self.length = end;
        Ok(())
    }
}

impl Default for CommandOutput {
    fn default() -> Self {
        Self::new()
    }
}
