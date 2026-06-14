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
//! - Filesystem registry state and descriptor operations (-> `service`)
//!
//! ## Public API
//! - [`initialize`] - Initialize the kernel filesystem namespace
//! - [`open`] - Open a path and return a file descriptor
//! - [`read`] - Read from an open file descriptor
//! - [`read_at`] - Read from an open file descriptor without changing its offset
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
pub use descriptor::{FileDescriptor, SeekWhence, STANDARD_ERROR, STANDARD_INPUT, STANDARD_OUTPUT};
#[allow(unused_imports)]
pub use mount::{MountFlags, MountInfo, MountSource};
#[allow(unused_imports)]
pub use node::{DirectoryEntry, FileMetadata, FileSystemError, FileSystemResult, FileType};
pub use ramfs::RamFile;
#[allow(unused_imports)]
pub use read_only::ReadOnlyFile;
pub use service::{
    close, descriptor_metadata, initialize, list_directory, list_mounts, metadata,
    mount_fat32_file, mount_ram_file, mount_read_only_file, normalize_path_for_display, open, read,
    read_at, read_directory, seek, seek_from, write,
};
