//! Filesystem service state and public facade functions.

use super::descriptor::{FileDescriptor, FileDescriptorTable};
use super::namespace::VirtualFileSystem;
use super::node::{normalize_path, DirectoryEntry, FileMetadata, FileNode, FileSystemResult};
use alloc::format;
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

    *FILE_DESCRIPTORS.lock() = create_standard_file_descriptor_table();
}

/// Create a descriptor table with standard input, output, and error installed.
///
/// # Panics
///
/// Panics if required built-in device nodes cannot be found after mounting.
pub fn create_standard_file_descriptor_table() -> FileDescriptorTable {
    let input = get_required_node("/dev/null", "standard input");
    let output = get_required_node("/dev/console", "standard output");
    let error = get_required_node("/dev/console", "standard error");
    let mut file_descriptors = FileDescriptorTable::new();
    file_descriptors.initialize_standard_descriptors(input, output, error);
    file_descriptors
}

fn get_required_node(path: &str, description: &str) -> Arc<dyn FileNode> {
    VIRTUAL_FILE_SYSTEM
        .lock()
        .get_node(path)
        .unwrap_or_else(|_| panic!("{description} device must exist at {path}"))
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
    open_with_close_on_exec(path, false)
}

/// Open a path with close-on-exec metadata and return a file descriptor.
pub fn open_with_close_on_exec(
    path: &str,
    close_on_exec: bool,
) -> FileSystemResult<FileDescriptor> {
    let mut file_descriptors = FILE_DESCRIPTORS.lock();
    open_with_close_on_exec_in(&mut file_descriptors, path, close_on_exec)
}

/// Open a path in the provided descriptor table.
pub fn open_with_close_on_exec_in(
    file_descriptors: &mut FileDescriptorTable,
    path: &str,
    close_on_exec: bool,
) -> FileSystemResult<FileDescriptor> {
    let node = VIRTUAL_FILE_SYSTEM.lock().get_node(path)?;
    if close_on_exec {
        file_descriptors.open_with_close_on_exec(node, true)
    } else {
        file_descriptors.open(node)
    }
}

/// Close an open file descriptor.
pub fn close(descriptor: FileDescriptor) -> FileSystemResult<()> {
    FILE_DESCRIPTORS.lock().close(descriptor)
}

/// Read bytes from an open file descriptor.
pub fn read(descriptor: FileDescriptor, buffer: &mut [u8]) -> FileSystemResult<usize> {
    FILE_DESCRIPTORS.lock().read(descriptor, buffer)
}

/// Write bytes to an open file descriptor.
pub fn write(descriptor: FileDescriptor, buffer: &[u8]) -> FileSystemResult<usize> {
    FILE_DESCRIPTORS.lock().write(descriptor, buffer)
}

/// Seek an open file descriptor to an absolute offset.
pub fn seek(descriptor: FileDescriptor, offset: usize) -> FileSystemResult<usize> {
    FILE_DESCRIPTORS.lock().seek(descriptor, offset)
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

/// Resolve a path against a current working directory and return a canonical path.
pub fn resolve_path(current_working_directory: &str, path: &str) -> alloc::string::String {
    if path.starts_with('/') {
        normalize_path(path)
    } else {
        normalize_path(&format!("{current_working_directory}/{path}"))
    }
}
