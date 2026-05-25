//! Open file descriptor table.

use crate::kernel::filesystem::node::{
    DirectoryEntry, FileMetadata, FileNode, FileSystemError, FileSystemResult,
};
use alloc::sync::Arc;
use alloc::vec::Vec;

/// Numeric handle for an open file.
pub type FileDescriptor = usize;

/// Standard input file descriptor.
pub const STANDARD_INPUT: FileDescriptor = 0;
/// Standard output file descriptor.
pub const STANDARD_OUTPUT: FileDescriptor = 1;
/// Standard error file descriptor.
pub const STANDARD_ERROR: FileDescriptor = 2;

const MAX_OPEN_FILES: usize = 64;

#[derive(Clone)]
struct OpenFile {
    node: Arc<dyn FileNode>,
    offset: usize,
}

/// Per-kernel open file descriptor table.
pub struct FileDescriptorTable {
    entries: Vec<Option<OpenFile>>,
}

impl FileDescriptorTable {
    /// Create an empty file descriptor table.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Install standard input, output, and error descriptors.
    pub fn initialize_standard_descriptors(
        &mut self,
        input: Arc<dyn FileNode>,
        output: Arc<dyn FileNode>,
        error: Arc<dyn FileNode>,
    ) {
        self.entries.clear();
        self.entries.push(Some(OpenFile {
            node: input,
            offset: 0,
        }));
        self.entries.push(Some(OpenFile {
            node: output,
            offset: 0,
        }));
        self.entries.push(Some(OpenFile {
            node: error,
            offset: 0,
        }));
    }

    /// Open a node and return a file descriptor.
    pub fn open(&mut self, node: Arc<dyn FileNode>) -> FileSystemResult<FileDescriptor> {
        for (descriptor, entry) in self.entries.iter_mut().enumerate() {
            if entry.is_none() {
                *entry = Some(OpenFile { node, offset: 0 });
                return Ok(descriptor);
            }
        }

        if self.entries.len() >= MAX_OPEN_FILES {
            return Err(FileSystemError::TooManyOpenFiles);
        }

        let descriptor = self.entries.len();
        self.entries.push(Some(OpenFile { node, offset: 0 }));
        Ok(descriptor)
    }

    /// Close an open file descriptor.
    pub fn close(&mut self, descriptor: FileDescriptor) -> FileSystemResult<()> {
        let Some(entry) = self.entries.get_mut(descriptor) else {
            return Err(FileSystemError::InvalidFileDescriptor);
        };

        if entry.is_none() {
            return Err(FileSystemError::InvalidFileDescriptor);
        }

        *entry = None;
        Ok(())
    }

    /// Read from an open file descriptor at its current offset.
    pub fn read(
        &mut self,
        descriptor: FileDescriptor,
        buffer: &mut [u8],
    ) -> FileSystemResult<usize> {
        let open_file = self.get_open_file_mut(descriptor)?;
        let count = open_file.node.read_at(open_file.offset, buffer)?;
        open_file.offset = open_file.offset.saturating_add(count);
        Ok(count)
    }

    /// Write to an open file descriptor at its current offset.
    pub fn write(&mut self, descriptor: FileDescriptor, buffer: &[u8]) -> FileSystemResult<usize> {
        let open_file = self.get_open_file_mut(descriptor)?;
        let count = open_file.node.write_at(open_file.offset, buffer)?;
        open_file.offset = open_file.offset.saturating_add(count);
        Ok(count)
    }

    /// Seek an open file descriptor and return the new offset.
    pub fn seek(&mut self, descriptor: FileDescriptor, offset: usize) -> FileSystemResult<usize> {
        let open_file = self.get_open_file_mut(descriptor)?;
        open_file.offset = offset;
        Ok(open_file.offset)
    }

    /// Return metadata for an open file descriptor.
    pub fn metadata(&mut self, descriptor: FileDescriptor) -> FileSystemResult<FileMetadata> {
        let open_file = self.get_open_file_mut(descriptor)?;
        Ok(open_file.node.metadata())
    }

    /// Read one directory entry from an open directory descriptor.
    pub fn read_directory(
        &mut self,
        descriptor: FileDescriptor,
    ) -> FileSystemResult<Option<DirectoryEntry>> {
        let open_file = self.get_open_file_mut(descriptor)?;
        let entries = open_file.node.list_entries()?;
        let entry = entries.get(open_file.offset).cloned();
        if entry.is_some() {
            open_file.offset = open_file.offset.saturating_add(1);
        }
        Ok(entry)
    }

    fn get_open_file_mut(&mut self, descriptor: FileDescriptor) -> FileSystemResult<&mut OpenFile> {
        self.entries
            .get_mut(descriptor)
            .and_then(Option::as_mut)
            .ok_or(FileSystemError::InvalidFileDescriptor)
    }
}
