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
const STANDARD_DESCRIPTOR_COUNT: usize = STANDARD_ERROR + 1;

/// Starting point for a seek operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeekWhence {
    /// Seek relative to the start of the file.
    Start,
    /// Seek relative to the current descriptor offset.
    Current,
    /// Seek relative to the end of the file.
    End,
}

#[derive(Clone)]
struct OpenFile {
    node: Arc<dyn FileNode>,
    offset: usize,
    close_on_exec: bool,
}

impl OpenFile {
    fn new(node: Arc<dyn FileNode>, close_on_exec: bool) -> Self {
        Self {
            node,
            offset: 0,
            close_on_exec,
        }
    }
}

/// Snapshot of the current spawn descriptor inheritance selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnDescriptorInheritanceSnapshot {
    inherited: usize,
    standard: usize,
    close_on_exec: usize,
}

impl SpawnDescriptorInheritanceSnapshot {
    const fn new(
        inherited_descriptors: usize,
        standard_descriptors: usize,
        close_on_exec_descriptors: usize,
    ) -> Self {
        Self {
            inherited: inherited_descriptors,
            standard: standard_descriptors,
            close_on_exec: close_on_exec_descriptors,
        }
    }

    /// Return the number of open descriptors selected for the current spawn policy.
    pub const fn inherited_descriptors(self) -> usize {
        self.inherited
    }

    /// Return the number of selected descriptors in the standard `0..=2` range.
    pub const fn standard_descriptors(self) -> usize {
        self.standard
    }

    /// Return the number of open descriptors excluded because they are close-on-exec.
    pub const fn close_on_exec_descriptors(self) -> usize {
        self.close_on_exec
    }
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
        self.entries.push(Some(OpenFile::new(input, false)));
        self.entries.push(Some(OpenFile::new(output, false)));
        self.entries.push(Some(OpenFile::new(error, false)));
    }

    /// Open a node and return a file descriptor.
    pub fn open(&mut self, node: Arc<dyn FileNode>) -> FileSystemResult<FileDescriptor> {
        self.open_with_close_on_exec(node, false)
    }

    /// Open a node with close-on-exec metadata and return a file descriptor.
    pub fn open_with_close_on_exec(
        &mut self,
        node: Arc<dyn FileNode>,
        close_on_exec: bool,
    ) -> FileSystemResult<FileDescriptor> {
        for (descriptor, entry) in self.entries.iter_mut().enumerate() {
            if entry.is_none() {
                *entry = Some(OpenFile::new(node, close_on_exec));
                return Ok(descriptor);
            }
        }

        if self.entries.len() >= MAX_OPEN_FILES {
            return Err(FileSystemError::TooManyOpenFiles);
        }

        let descriptor = self.entries.len();
        self.entries.push(Some(OpenFile::new(node, close_on_exec)));
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

    /// Close all descriptors marked close-on-exec and return the number closed.
    pub fn close_on_exec_descriptors(&mut self) -> usize {
        let mut closed_count = 0;
        for entry in &mut self.entries {
            if entry
                .as_ref()
                .is_some_and(|open_file| open_file.close_on_exec)
            {
                *entry = None;
                closed_count += 1;
            }
        }
        closed_count
    }

    /// Return the current descriptor set selected for spawn inheritance.
    pub fn get_spawn_descriptor_inheritance_snapshot(&self) -> SpawnDescriptorInheritanceSnapshot {
        let mut inherited_descriptors = 0_usize;
        let mut standard_descriptors = 0_usize;
        let mut close_on_exec_descriptors = 0_usize;
        for (descriptor, entry) in self.entries.iter().enumerate() {
            let Some(open_file) = entry.as_ref() else {
                continue;
            };
            if open_file.close_on_exec {
                close_on_exec_descriptors = close_on_exec_descriptors.saturating_add(1);
                continue;
            }
            inherited_descriptors = inherited_descriptors.saturating_add(1);
            if descriptor < STANDARD_DESCRIPTOR_COUNT {
                standard_descriptors = standard_descriptors.saturating_add(1);
            }
        }
        SpawnDescriptorInheritanceSnapshot::new(
            inherited_descriptors,
            standard_descriptors,
            close_on_exec_descriptors,
        )
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

    /// Read from an open file descriptor at an absolute offset.
    pub fn read_at(
        &self,
        descriptor: FileDescriptor,
        offset: usize,
        buffer: &mut [u8],
    ) -> FileSystemResult<usize> {
        let open_file = self.get_open_file(descriptor)?;
        open_file.node.read_at(offset, buffer)
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

    /// Seek an open file descriptor relative to the requested base.
    pub fn seek_from(
        &mut self,
        descriptor: FileDescriptor,
        offset: i64,
        whence: SeekWhence,
    ) -> FileSystemResult<usize> {
        let open_file = self.get_open_file_mut(descriptor)?;
        let base = match whence {
            SeekWhence::Start => 0,
            SeekWhence::Current => open_file.offset,
            SeekWhence::End => open_file.node.metadata().size,
        };
        let next_offset =
            add_signed_offset(base, offset).ok_or(FileSystemError::InvalidArgument)?;
        open_file.offset = next_offset;
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

    fn get_open_file(&self, descriptor: FileDescriptor) -> FileSystemResult<&OpenFile> {
        self.entries
            .get(descriptor)
            .and_then(Option::as_ref)
            .ok_or(FileSystemError::InvalidFileDescriptor)
    }
}

fn add_signed_offset(base: usize, offset: i64) -> Option<usize> {
    if offset >= 0 {
        base.checked_add(usize::try_from(offset).ok()?)
    } else {
        base.checked_sub(usize::try_from(offset.checked_abs()?).ok()?)
    }
}
