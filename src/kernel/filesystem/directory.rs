//! Directory filesystem nodes.

use crate::kernel::filesystem::node::{
    DirectoryEntry, FileMetadata, FileNode, FileSystemError, FileSystemResult, FileType,
};
use alloc::vec::Vec;
use spin::Mutex;

/// Read-only directory node backed by a snapshot of directory entries.
pub struct DirectoryNode {
    entries: Mutex<Vec<DirectoryEntry>>,
}

impl DirectoryNode {
    /// Create an empty directory node.
    pub fn empty() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Replace this directory's listing.
    pub fn set_entries(&self, entries: Vec<DirectoryEntry>) {
        *self.entries.lock() = entries;
    }
}

impl FileNode for DirectoryNode {
    fn read_at(&self, _offset: usize, _buffer: &mut [u8]) -> FileSystemResult<usize> {
        Err(FileSystemError::IsDirectory)
    }

    fn write_at(&self, _offset: usize, _buffer: &[u8]) -> FileSystemResult<usize> {
        Err(FileSystemError::UnsupportedOperation)
    }

    fn metadata(&self) -> FileMetadata {
        FileMetadata {
            file_type: FileType::Directory,
            size: self.entries.lock().len(),
            writable: false,
        }
    }

    fn list_entries(&self) -> FileSystemResult<Vec<DirectoryEntry>> {
        Ok(self.entries.lock().clone())
    }
}
