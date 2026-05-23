//! # `kernel::filesystem`
//!
//! ## Owns
//! - Kernel virtual filesystem registry
//! - Memory-backed files
//! - Device nodes for `/dev/console` and `/dev/null`
//! - Kernel file descriptor table
//!
//! ## Does NOT own
//! - Real block storage drivers
//! - Filesystem parsers for on-disk formats
//! - User pointer validation for syscalls
//!
//! ## Public API
//! - [`initialize`] - Initialize the kernel filesystem namespace
//! - [`open`] - Open a path and return a file descriptor
//! - [`read`] - Read from an open file descriptor
//! - [`write`] - Write to an open file descriptor
//! - [`close`] - Close an open file descriptor

mod descriptor;
mod device;
mod node;
mod ramfs;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use descriptor::FileDescriptorTable;
use spin::{LazyLock, Mutex};

pub use descriptor::{FileDescriptor, STANDARD_ERROR, STANDARD_INPUT, STANDARD_OUTPUT};
pub use node::{FileSystemError, FileSystemResult};
pub use ramfs::RamFile;

use node::{normalize_path, FileNode};

static VIRTUAL_FILE_SYSTEM: LazyLock<Mutex<VirtualFileSystem>> =
    LazyLock::new(|| Mutex::new(VirtualFileSystem::new()));
static FILE_DESCRIPTORS: LazyLock<Mutex<FileDescriptorTable>> =
    LazyLock::new(|| Mutex::new(FileDescriptorTable::new()));

struct VirtualFileSystem {
    nodes: BTreeMap<String, Arc<dyn FileNode>>,
    initialized: bool,
}

impl VirtualFileSystem {
    fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            initialized: false,
        }
    }

    fn initialize(&mut self) -> FileSystemResult<()> {
        if self.initialized {
            return Err(FileSystemError::AlreadyInitialized);
        }

        self.mount_node("/dev/console", Arc::new(device::ConsoleDevice::new()));
        self.mount_node("/dev/null", Arc::new(device::NullDevice::new()));
        self.mount_node(
            "/README",
            Arc::new(RamFile::from_bytes(b"ManaOS ramfs is initialized.\n")),
        );
        self.initialized = true;
        Ok(())
    }

    fn mount_node(&mut self, path: &str, node: Arc<dyn FileNode>) {
        self.nodes.insert(normalize_path(path), node);
    }

    fn get_node(&self, path: &str) -> FileSystemResult<Arc<dyn FileNode>> {
        self.nodes
            .get(&normalize_path(path))
            .cloned()
            .ok_or(FileSystemError::NotFound)
    }
}

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

    let input = get_node("/dev/null").expect("standard input device must exist");
    let output = get_node("/dev/console").expect("standard output device must exist");
    let error = get_node("/dev/console").expect("standard error device must exist");
    FILE_DESCRIPTORS
        .lock()
        .initialize_standard_descriptors(input, output, error);
}

/// Mount a memory-backed file at an absolute path.
pub fn mount_ram_file(path: &str, contents: &[u8]) {
    VIRTUAL_FILE_SYSTEM
        .lock()
        .mount_node(path, Arc::new(RamFile::from_bytes(contents)));
}

/// Open a path and return a file descriptor.
pub fn open(path: &str) -> FileSystemResult<FileDescriptor> {
    let node = get_node(path)?;
    FILE_DESCRIPTORS.lock().open(node)
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

fn get_node(path: &str) -> FileSystemResult<Arc<dyn FileNode>> {
    VIRTUAL_FILE_SYSTEM.lock().get_node(path)
}
