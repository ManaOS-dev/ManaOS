//! Virtual filesystem mount metadata.

use alloc::string::String;

/// Mount source kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MountSource {
    /// In-memory writable filesystem.
    Ram,
    /// Device namespace.
    Device,
    /// Read-only FAT32-backed namespace snapshot.
    Fat32,
}

/// Mount access flags.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MountFlags {
    /// Whether writes are allowed through this mount.
    pub writable: bool,
}

impl MountFlags {
    /// Create read-only mount flags.
    pub const fn read_only() -> Self {
        Self { writable: false }
    }

    /// Create read-write mount flags.
    pub const fn read_write() -> Self {
        Self { writable: true }
    }
}

/// Mounted namespace metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MountInfo {
    /// Mount point path.
    pub path: String,
    /// Mount source kind.
    pub source: MountSource,
    /// Mount access flags.
    pub flags: MountFlags,
}
