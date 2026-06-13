//! Shared `ManaOS` syscall ABI constants and fixed user records.

/// Linux-compatible read syscall number.
pub const SYS_READ: u64 = 0;
/// Linux-compatible write syscall number.
pub const SYS_WRITE: u64 = 1;
/// Linux-compatible open syscall number.
pub const SYS_OPEN: u64 = 2;
/// Linux-compatible close syscall number.
pub const SYS_CLOSE: u64 = 3;
/// Linux-compatible file status syscall number.
pub const SYS_FSTAT: u64 = 5;
/// Linux-compatible seek syscall number.
pub const SYS_LSEEK: u64 = 8;
/// Linux-compatible anonymous memory-map syscall number.
pub const SYS_MMAP: u64 = 9;
/// Linux-compatible memory-unmap syscall number.
pub const SYS_MUNMAP: u64 = 11;
/// Linux-compatible heap break syscall number.
pub const SYS_BRK: u64 = 12;
/// Linux-compatible get-process-identifier syscall number.
pub const SYS_GETPID: u64 = 39;
/// Linux-compatible exit syscall number.
pub const SYS_EXIT: u64 = 60;
/// Linux-compatible get-directory-entries syscall number.
pub const SYS_GETDENTS64: u64 = 217;
/// Linux-compatible exit-group syscall number.
pub const SYS_EXIT_GROUP: u64 = 231;
/// Linux-compatible open-at syscall number.
pub const SYS_OPENAT: u64 = 257;

/// File opened for read-only access.
pub const OPEN_READ_ONLY: u64 = 0;
/// Linux-compatible current-working-directory marker for `openat`.
pub const AT_FDCWD: u64 = u64::MAX - 99;
/// Seek relative to the start of a file.
pub const SEEK_SET: u64 = 0;
/// Seek relative to the current file offset.
pub const SEEK_CUR: u64 = 1;
/// Seek relative to the end of a file.
pub const SEEK_END: u64 = 2;

/// Mapping pages may be read by user code.
pub const PROT_READ: u64 = 0x1;
/// Mapping pages may be written by user code.
pub const PROT_WRITE: u64 = 0x2;
/// Mapping pages may be executed by user code.
pub const PROT_EXEC: u64 = 0x4;
/// Mapping is private to the current process.
pub const MAP_PRIVATE: u64 = 0x02;
/// Mapping is anonymous and not backed by a file descriptor.
pub const MAP_ANONYMOUS: u64 = 0x20;
/// Fixed mapping must fail when the requested range is already mapped.
pub const MAP_FIXED_NOREPLACE: u64 = 0x0010_0000;

/// File status type for a regular file.
pub const FILE_TYPE_REGULAR: u64 = 1;
/// File status type for a directory.
pub const FILE_TYPE_DIRECTORY: u64 = 2;
/// File status type for a device.
pub const FILE_TYPE_DEVICE: u64 = 3;
/// Maximum file name bytes stored in one fixed directory-entry record.
pub const DIRECTORY_ENTRY_NAME_BYTES: usize = 56;

/// Linux-compatible not found error as a signed syscall result.
pub const ERROR_NOT_FOUND: isize = -2;
/// Linux-compatible bad file descriptor error as a signed syscall result.
pub const ERROR_BAD_FILE_DESCRIPTOR: isize = -9;
/// Bad address error return value as a signed syscall result.
pub const ERROR_BAD_ADDRESS: isize = -14;
/// Linux-compatible file exists error as a signed syscall result.
pub const ERROR_FILE_EXISTS: isize = -17;
/// Linux-compatible not implemented error as a signed syscall result.
pub const ERROR_NOT_IMPLEMENTED: isize = -38;

/// Metadata returned by the `ManaOS` `fstat` syscall.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UserFileStat {
    /// File node kind encoded as a `FILE_TYPE_*` value.
    pub file_type: u64,
    /// File size in bytes.
    pub size: u64,
    /// Non-zero when the file descriptor supports writes.
    pub writable: u64,
}

impl UserFileStat {
    /// Create an empty metadata record for use as an output buffer.
    pub const fn empty() -> Self {
        Self {
            file_type: 0,
            size: 0,
            writable: 0,
        }
    }
}

/// Fixed-size directory entry returned by the `ManaOS` `getdents64` syscall.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UserDirectoryEntry {
    /// File node kind encoded as a `FILE_TYPE_*` value.
    pub file_type: u64,
    /// File size in bytes.
    pub size: u64,
    /// Number of valid bytes in [`Self::name`].
    pub name_length: u64,
    /// UTF-8 entry name bytes, truncated only if the source exceeds the fixed record.
    pub name: [u8; DIRECTORY_ENTRY_NAME_BYTES],
}

impl UserDirectoryEntry {
    /// Create an empty directory-entry record for use as an output buffer.
    pub const fn empty() -> Self {
        Self {
            file_type: 0,
            size: 0,
            name_length: 0,
            name: [0; DIRECTORY_ENTRY_NAME_BYTES],
        }
    }
}
