//! Read-only virtual filesystem nodes backed by external storage callbacks.

use super::node::{FileMetadata, FileNode, FileSystemError, FileSystemResult, FileType};

/// External read callback for a read-only backend file.
pub type BackendRead = fn(context: usize, offset: usize, buffer: &mut [u8]) -> Option<usize>;

/// Read-only file node whose contents are supplied by another subsystem.
pub struct ReadOnlyBackendFile {
    size: usize,
    context: usize,
    read: BackendRead,
}

impl ReadOnlyBackendFile {
    /// Create a read-only backend file with a fixed visible size.
    pub fn new(size: usize, context: usize, read: BackendRead) -> Self {
        Self {
            size,
            context,
            read,
        }
    }
}

impl FileNode for ReadOnlyBackendFile {
    fn read_at(&self, offset: usize, buffer: &mut [u8]) -> FileSystemResult<usize> {
        (self.read)(self.context, offset, buffer).ok_or(FileSystemError::UnsupportedOperation)
    }

    fn write_at(&self, _offset: usize, _buffer: &[u8]) -> FileSystemResult<usize> {
        Err(FileSystemError::UnsupportedOperation)
    }

    fn metadata(&self) -> FileMetadata {
        FileMetadata {
            file_type: FileType::Regular,
            size: self.size,
            writable: false,
        }
    }
}
