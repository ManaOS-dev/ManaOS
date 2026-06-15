//! # `kernel::filesystem`
//!
//! ## Owns
//! - Kernel virtual filesystem module composition
//! - Public filesystem type and service re-exports
//!
//! ## Does NOT own
//! - Real block storage drivers
//! - Filesystem parsers for on-disk formats
//! - User pointer validation for syscalls
//! - Filesystem registry state and kernel descriptor operations (-> `service`)
//! - Process descriptor table ownership (-> `kernel::task`)
//!
//! ## Public API
//! - [`initialize`] - Initialize the kernel filesystem namespace
//! - [`create_standard_file_descriptor_table`] - Build a standard descriptor table
//! - [`open`] - Open a path and return a file descriptor
//! - [`read`] - Read from an open file descriptor
//! - [`write`] - Write to an open file descriptor
//! - [`close`] - Close an open file descriptor

mod backend;
mod descriptor;
mod device;
mod directory;
mod mount;
mod namespace;
mod node;
mod ramfs;
mod read_only;
mod service;

#[allow(unused_imports)]
pub use backend::{BackendRead, ReadOnlyBackendFile};
#[allow(unused_imports)]
pub use descriptor::{
    FileDescriptor, FileDescriptorTable, SeekWhence, SpawnDescriptorInheritanceSnapshot,
    STANDARD_ERROR, STANDARD_INPUT, STANDARD_OUTPUT,
};
#[allow(unused_imports)]
pub use mount::{MountFlags, MountInfo, MountSource};
#[allow(unused_imports)]
pub use node::{DirectoryEntry, FileMetadata, FileSystemError, FileSystemResult, FileType};
pub use ramfs::RamFile;
#[allow(unused_imports)]
pub use read_only::ReadOnlyFile;
#[allow(unused_imports)]
pub use service::{
    close, create_standard_file_descriptor_table, descriptor_metadata, initialize, list_directory,
    list_mounts, metadata, mount_fat32_file, mount_ram_file, mount_read_only_file,
    normalize_path_for_display, open, open_with_close_on_exec, open_with_close_on_exec_in, read,
    read_directory, resolve_path, seek, write,
};
