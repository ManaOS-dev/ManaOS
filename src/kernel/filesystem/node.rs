//! Filesystem node contracts.

use alloc::string::String;

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
}

/// A file-like object addressable through the virtual filesystem.
pub trait FileNode: Send + Sync {
    /// Read bytes at an absolute file offset.
    fn read_at(&self, offset: usize, buffer: &mut [u8]) -> FileSystemResult<usize>;

    /// Write bytes at an absolute file offset.
    fn write_at(&self, offset: usize, buffer: &[u8]) -> FileSystemResult<usize>;
}

/// Normalize an absolute path into the canonical filesystem key.
pub fn normalize_path(path: &str) -> String {
    if path == "/" {
        return String::from("/");
    }

    let mut normalized = String::new();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }

        normalized.push('/');
        normalized.push_str(segment);
    }

    if normalized.is_empty() {
        String::from("/")
    } else {
        normalized
    }
}
