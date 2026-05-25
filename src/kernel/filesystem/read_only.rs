//! Read-only filesystem nodes.

use crate::kernel::filesystem::node::{FileNode, FileSystemError, FileSystemResult};
use alloc::vec::Vec;

/// A read-only memory-backed regular file.
pub struct ReadOnlyFile {
    data: Vec<u8>,
}

impl ReadOnlyFile {
    /// Create a read-only file initialized with bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: Vec::from(bytes),
        }
    }
}

impl FileNode for ReadOnlyFile {
    fn read_at(&self, offset: usize, buffer: &mut [u8]) -> FileSystemResult<usize> {
        if offset >= self.data.len() {
            return Ok(0);
        }

        let available = self.data.len() - offset;
        let count = available.min(buffer.len());
        buffer[..count].copy_from_slice(&self.data[offset..offset + count]);
        Ok(count)
    }

    fn write_at(&self, _offset: usize, _buffer: &[u8]) -> FileSystemResult<usize> {
        Err(FileSystemError::UnsupportedOperation)
    }
}
