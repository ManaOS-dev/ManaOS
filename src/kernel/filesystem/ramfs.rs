//! Memory-backed filesystem nodes.

use crate::kernel::filesystem::node::{FileMetadata, FileNode, FileSystemResult, FileType};
use alloc::vec::Vec;
use spin::Mutex;

/// A memory-backed regular file.
pub struct RamFile {
    data: Mutex<Vec<u8>>,
}

impl RamFile {
    /// Create a memory-backed file initialized with bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: Mutex::new(Vec::from(bytes)),
        }
    }
}

impl FileNode for RamFile {
    fn read_at(&self, offset: usize, buffer: &mut [u8]) -> FileSystemResult<usize> {
        let data = self.data.lock();
        if offset >= data.len() {
            return Ok(0);
        }

        let available = data.len() - offset;
        let count = available.min(buffer.len());
        buffer[..count].copy_from_slice(&data[offset..offset + count]);
        Ok(count)
    }

    fn write_at(&self, offset: usize, buffer: &[u8]) -> FileSystemResult<usize> {
        let mut data = self.data.lock();
        let required_length = offset.saturating_add(buffer.len());
        if data.len() < required_length {
            data.resize(required_length, 0);
        }

        data[offset..offset + buffer.len()].copy_from_slice(buffer);
        Ok(buffer.len())
    }

    fn metadata(&self) -> FileMetadata {
        FileMetadata {
            file_type: FileType::Regular,
            size: self.data.lock().len(),
            writable: true,
        }
    }
}
