//! Device-backed filesystem nodes.

use crate::kernel::filesystem::node::{
    FileMetadata, FileNode, FileSystemError, FileSystemResult, FileType,
};
use core::str;

/// Serial-backed console device.
pub struct ConsoleDevice;

impl ConsoleDevice {
    /// Create a console device node.
    pub const fn new() -> Self {
        Self
    }
}

impl FileNode for ConsoleDevice {
    fn read_at(&self, _offset: usize, _buffer: &mut [u8]) -> FileSystemResult<usize> {
        Err(FileSystemError::UnsupportedOperation)
    }

    fn write_at(&self, _offset: usize, buffer: &[u8]) -> FileSystemResult<usize> {
        if let Ok(text) = str::from_utf8(buffer) {
            crate::serial_print!("{text}");
        } else {
            for byte in buffer {
                crate::serial_print!("{byte:02x}");
            }
        }

        Ok(buffer.len())
    }

    fn metadata(&self) -> FileMetadata {
        FileMetadata {
            file_type: FileType::Device,
            size: 0,
            writable: true,
        }
    }
}

/// Null device that discards writes and returns end-of-file on reads.
pub struct NullDevice;

impl NullDevice {
    /// Create a null device node.
    pub const fn new() -> Self {
        Self
    }
}

impl FileNode for NullDevice {
    fn read_at(&self, _offset: usize, _buffer: &mut [u8]) -> FileSystemResult<usize> {
        Ok(0)
    }

    fn write_at(&self, _offset: usize, buffer: &[u8]) -> FileSystemResult<usize> {
        Ok(buffer.len())
    }

    fn metadata(&self) -> FileMetadata {
        FileMetadata {
            file_type: FileType::Device,
            size: 0,
            writable: true,
        }
    }
}
