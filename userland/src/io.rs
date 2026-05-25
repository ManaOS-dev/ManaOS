//! Small file-descriptor helpers for ManaOS user programs.

use crate::syscall;

/// Standard input file descriptor.
pub const STANDARD_INPUT: usize = 0;
/// Standard output file descriptor.
pub const STANDARD_OUTPUT: usize = 1;
/// Standard error file descriptor.
pub const STANDARD_ERROR: usize = 2;
/// Maximum path bytes accepted by the fixed userland path buffer.
pub const PATH_BUFFER_BYTES: usize = 128;

/// Error returned by userland I/O helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoError {
    /// The path did not fit in the fixed path buffer.
    PathTooLong,
    /// The kernel returned a negative syscall result.
    Syscall(isize),
}

/// Open file descriptor wrapper.
#[derive(Debug, Eq, PartialEq)]
pub struct FileDescriptor {
    descriptor: usize,
}

impl FileDescriptor {
    /// Open `path` as a read-only file.
    pub fn open_read_only(path: &str) -> Result<Self, IoError> {
        let mut path_buffer = [0_u8; PATH_BUFFER_BYTES];
        let path = copy_path(path, &mut path_buffer)?;
        let descriptor = syscall::open(path);
        if descriptor < 0 {
            return Err(IoError::Syscall(descriptor));
        }

        Ok(Self {
            descriptor: descriptor as usize,
        })
    }

    /// Return the raw descriptor value.
    pub fn raw(&self) -> usize {
        self.descriptor
    }

    /// Read bytes from this descriptor.
    pub fn read(&self, buffer: &mut [u8]) -> Result<usize, IoError> {
        let bytes_read = syscall::read(self.descriptor, buffer);
        if bytes_read < 0 {
            return Err(IoError::Syscall(bytes_read));
        }

        Ok(bytes_read as usize)
    }

    /// Write bytes to this descriptor.
    pub fn write(&self, buffer: &[u8]) -> Result<usize, IoError> {
        let bytes_written = syscall::write(self.descriptor, buffer);
        if bytes_written < 0 {
            return Err(IoError::Syscall(bytes_written));
        }

        Ok(bytes_written as usize)
    }

    /// Close this descriptor.
    pub fn close(self) -> Result<(), IoError> {
        let result = syscall::close(self.descriptor);
        if result < 0 {
            return Err(IoError::Syscall(result));
        }

        Ok(())
    }
}

/// Write all bytes to standard output.
pub fn write_stdout(buffer: &[u8]) -> Result<usize, IoError> {
    FileDescriptor {
        descriptor: STANDARD_OUTPUT,
    }
    .write(buffer)
}

fn copy_path<'a>(path: &str, buffer: &'a mut [u8; PATH_BUFFER_BYTES]) -> Result<&'a [u8], IoError> {
    let bytes = path.as_bytes();
    if bytes.len().saturating_add(1) > buffer.len() {
        return Err(IoError::PathTooLong);
    }

    buffer[..bytes.len()].copy_from_slice(bytes);
    buffer[bytes.len()] = 0;
    Ok(&buffer[..=bytes.len()])
}
