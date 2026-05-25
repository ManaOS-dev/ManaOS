//! Filesystem node contracts.

use alloc::string::String;
use alloc::vec::Vec;

/// Result type used by filesystem operations.
pub type FileSystemResult<T> = Result<T, FileSystemError>;

/// Filesystem operation error.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FileSystemError {
    /// The requested path does not exist.
    NotFound,
    /// The file descriptor does not refer to an open file.
    InvalidFileDescriptor,
    /// The operation is not supported by this node.
    UnsupportedOperation,
    /// No file descriptor slot is available.
    TooManyOpenFiles,
    /// The filesystem has already been initialized.
    AlreadyInitialized,
    /// The requested path is not a directory.
    NotDirectory,
    /// The requested path is a directory.
    IsDirectory,
    /// The path is not valid for this filesystem.
    InvalidPath,
}

/// File node kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum FileType {
    /// Regular byte-addressable file.
    Regular,
    /// Directory containing named entries.
    Directory,
    /// Character or pseudo device.
    Device,
}

/// File metadata returned by the virtual filesystem.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FileMetadata {
    /// Node kind.
    pub file_type: FileType,
    /// File size in bytes.
    pub size: usize,
    /// Whether write operations are allowed.
    pub writable: bool,
}

/// Directory entry returned by read-only directory listing APIs.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DirectoryEntry {
    /// Entry name relative to the containing directory.
    pub name: String,
    /// Entry metadata.
    pub metadata: FileMetadata,
}

/// A file-like object addressable through the virtual filesystem.
pub trait FileNode: Send + Sync {
    /// Read bytes at an absolute file offset.
    fn read_at(&self, offset: usize, buffer: &mut [u8]) -> FileSystemResult<usize>;

    /// Write bytes at an absolute file offset.
    fn write_at(&self, offset: usize, buffer: &[u8]) -> FileSystemResult<usize>;

    /// Return metadata for this node.
    fn metadata(&self) -> FileMetadata {
        FileMetadata {
            file_type: FileType::Regular,
            size: 0,
            writable: true,
        }
    }

    /// Return directory entries for this node.
    fn list_entries(&self) -> FileSystemResult<Vec<DirectoryEntry>> {
        Err(FileSystemError::NotDirectory)
    }
}

/// Normalize an absolute path into the canonical filesystem key.
pub fn normalize_path(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    let mut normalized = String::new();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            let _ = segments.pop();
            continue;
        }

        segments.push(segment);
    }

    for segment in segments {
        normalized.push('/');
        normalized.push_str(segment);
    }

    if normalized.is_empty() {
        String::from("/")
    } else {
        normalized
    }
}
