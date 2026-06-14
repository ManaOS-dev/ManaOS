//! Filesystem service state and public facade functions.

use super::descriptor::{FileDescriptor, FileDescriptorTable, SeekWhence};
use super::namespace::VirtualFileSystem;
use super::node::{normalize_path, DirectoryEntry, FileMetadata, FileSystemResult};
use alloc::sync::Arc;
use spin::{LazyLock, Mutex};

use super::backend::{BackendRead, ReadOnlyBackendFile};
use super::mount::{MountFlags, MountInfo, MountSource};
use super::ramfs::RamFile;
use super::read_only::ReadOnlyFile;

static VIRTUAL_FILE_SYSTEM: LazyLock<Mutex<VirtualFileSystem>> =
    LazyLock::new(|| Mutex::new(VirtualFileSystem::new()));
static FILE_DESCRIPTORS: LazyLock<Mutex<FileDescriptorTable>> =
    LazyLock::new(|| Mutex::new(FileDescriptorTable::new()));

/// Initialize the kernel filesystem namespace and standard descriptors.
///
/// # Panics
///
/// Panics if required built-in device nodes cannot be found after mounting.
pub fn initialize() {
    {
        let mut virtual_file_system = VIRTUAL_FILE_SYSTEM.lock();
        if let Err(error) = virtual_file_system.initialize() {
            panic!("failed to initialize kernel filesystem: {error:?}");
        }
    }

    let input = VIRTUAL_FILE_SYSTEM
        .lock()
        .get_node("/dev/null")
        .expect("standard input device must exist");
    let output = VIRTUAL_FILE_SYSTEM
        .lock()
        .get_node("/dev/console")
        .expect("standard output device must exist");
    let error = VIRTUAL_FILE_SYSTEM
        .lock()
        .get_node("/dev/console")
        .expect("standard error device must exist");
    FILE_DESCRIPTORS
        .lock()
        .initialize_standard_descriptors(input, output, error);
}

/// Mount a memory-backed file at an absolute path.
pub fn mount_ram_file(path: &str, contents: &[u8]) {
    VIRTUAL_FILE_SYSTEM.lock().mount_node(
        path,
        Arc::new(RamFile::from_bytes(contents)),
        MountSource::Ram,
        MountFlags::read_write(),
    );
}

/// Mount a read-only memory-backed file at an absolute path.
pub fn mount_read_only_file(path: &str, contents: &[u8]) {
    VIRTUAL_FILE_SYSTEM.lock().mount_node(
        path,
        Arc::new(ReadOnlyFile::from_bytes(contents)),
        MountSource::Ram,
        MountFlags::read_only(),
    );
}

/// Mount a FAT32-backed read-only file at an absolute path.
pub fn mount_fat32_file(path: &str, size: usize, context: usize, read: BackendRead) {
    VIRTUAL_FILE_SYSTEM.lock().mount_node(
        path,
        Arc::new(ReadOnlyBackendFile::new(size, context, read)),
        MountSource::Fat32,
        MountFlags::read_only(),
    );
}

/// Open a path and return a file descriptor.
pub fn open(path: &str) -> FileSystemResult<FileDescriptor> {
    let node = VIRTUAL_FILE_SYSTEM.lock().get_node(path)?;
    FILE_DESCRIPTORS.lock().open(node)
}

/// Open a path with close-on-exec metadata and return a file descriptor.
pub fn open_with_close_on_exec(
    path: &str,
    close_on_exec: bool,
) -> FileSystemResult<FileDescriptor> {
    let node = VIRTUAL_FILE_SYSTEM.lock().get_node(path)?;
    FILE_DESCRIPTORS
        .lock()
        .open_with_close_on_exec(node, close_on_exec)
}

/// Close an open file descriptor.
pub fn close(descriptor: FileDescriptor) -> FileSystemResult<()> {
    FILE_DESCRIPTORS.lock().close(descriptor)
}

/// Close descriptors marked close-on-exec and return the number closed.
pub fn close_on_exec_descriptors() -> usize {
    FILE_DESCRIPTORS.lock().close_on_exec_descriptors()
}

/// Read bytes from an open file descriptor.
pub fn read(descriptor: FileDescriptor, buffer: &mut [u8]) -> FileSystemResult<usize> {
    FILE_DESCRIPTORS.lock().read(descriptor, buffer)
}

/// Read bytes from an open file descriptor without changing its current offset.
pub fn read_at(
    descriptor: FileDescriptor,
    offset: usize,
    buffer: &mut [u8],
) -> FileSystemResult<usize> {
    FILE_DESCRIPTORS.lock().read_at(descriptor, offset, buffer)
}

/// Write bytes to an open file descriptor.
pub fn write(descriptor: FileDescriptor, buffer: &[u8]) -> FileSystemResult<usize> {
    FILE_DESCRIPTORS.lock().write(descriptor, buffer)
}

/// Seek an open file descriptor to an absolute offset.
pub fn seek(descriptor: FileDescriptor, offset: usize) -> FileSystemResult<usize> {
    FILE_DESCRIPTORS.lock().seek(descriptor, offset)
}

/// Seek an open file descriptor relative to a base position.
pub fn seek_from(
    descriptor: FileDescriptor,
    offset: i64,
    whence: SeekWhence,
) -> FileSystemResult<usize> {
    FILE_DESCRIPTORS
        .lock()
        .seek_from(descriptor, offset, whence)
}

/// Return metadata for a filesystem path.
pub fn metadata(path: &str) -> FileSystemResult<FileMetadata> {
    VIRTUAL_FILE_SYSTEM.lock().metadata(path)
}

/// Return metadata for an open file descriptor.
pub fn descriptor_metadata(descriptor: FileDescriptor) -> FileSystemResult<FileMetadata> {
    FILE_DESCRIPTORS.lock().metadata(descriptor)
}

/// Read the next directory entry from an open directory descriptor.
pub fn read_directory(descriptor: FileDescriptor) -> FileSystemResult<Option<DirectoryEntry>> {
    FILE_DESCRIPTORS.lock().read_directory(descriptor)
}

/// List directory entries for a path.
pub fn list_directory(path: &str) -> FileSystemResult<alloc::vec::Vec<DirectoryEntry>> {
    VIRTUAL_FILE_SYSTEM.lock().list_directory(path)
}

/// Return mounted namespace metadata.
pub fn list_mounts() -> alloc::vec::Vec<MountInfo> {
    VIRTUAL_FILE_SYSTEM.lock().list_mounts()
}

/// Normalize a user-visible filesystem path for command output.
pub fn normalize_path_for_display(path: &str) -> alloc::string::String {
    normalize_path(path)
}
