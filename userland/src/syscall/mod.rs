//! # `mana_userland::syscall`
//!
//! ## Owns
//! - Linux-like ManaOS syscall numbers
//! - Safe-ish thin wrappers around raw syscall instructions
//!
//! ## Does NOT own
//! - Kernel syscall dispatch
//! - File descriptor lifetime policy
//! - Userland command parsing
//!
//! ## Public API
//! - [`read`] - Read bytes from an open file descriptor
//! - [`write`] - Write bytes to an open file descriptor
//! - [`open`] - Open a null-terminated path
//! - [`fstat`] - Read metadata for an open file descriptor
//! - [`lseek`] - Seek an open file descriptor
//! - [`brk`] - Move or query the user heap break
//! - [`exit`] - Terminate the current user task

#[path = "../../../src/shared/syscall_contract.rs"]
mod contract;
mod raw;

pub use contract::{UserDirectoryEntry, UserFileStat as FileStat, DIRECTORY_ENTRY_NAME_BYTES};
pub use raw::{syscall1, syscall2, syscall3, syscall4};

/// Linux-compatible read syscall number.
pub const SYS_READ: usize = contract::SYS_READ as usize;
/// Linux-compatible write syscall number.
pub const SYS_WRITE: usize = contract::SYS_WRITE as usize;
/// Linux-compatible open syscall number.
pub const SYS_OPEN: usize = contract::SYS_OPEN as usize;
/// Linux-compatible close syscall number.
pub const SYS_CLOSE: usize = contract::SYS_CLOSE as usize;
/// Linux-compatible file status syscall number.
pub const SYS_FSTAT: usize = contract::SYS_FSTAT as usize;
/// Linux-compatible seek syscall number.
pub const SYS_LSEEK: usize = contract::SYS_LSEEK as usize;
/// Linux-compatible heap break syscall number.
pub const SYS_BRK: usize = contract::SYS_BRK as usize;
/// Linux-compatible get-process-identifier syscall number.
pub const SYS_GETPID: usize = contract::SYS_GETPID as usize;
/// Linux-compatible exit syscall number.
pub const SYS_EXIT: usize = contract::SYS_EXIT as usize;
/// Linux-compatible get-directory-entries syscall number.
pub const SYS_GETDENTS64: usize = contract::SYS_GETDENTS64 as usize;
/// Linux-compatible exit-group syscall number.
pub const SYS_EXIT_GROUP: usize = contract::SYS_EXIT_GROUP as usize;
/// Linux-compatible open-at syscall number.
pub const SYS_OPENAT: usize = contract::SYS_OPENAT as usize;

/// File opened for read-only access.
pub const OPEN_READ_ONLY: usize = contract::OPEN_READ_ONLY as usize;
/// Linux-compatible current-working-directory marker for `openat`.
pub const AT_FDCWD: usize = contract::AT_FDCWD as usize;
/// Seek relative to the start of a file.
pub const SEEK_SET: usize = contract::SEEK_SET as usize;
/// Seek relative to the current file offset.
pub const SEEK_CUR: usize = contract::SEEK_CUR as usize;
/// Seek relative to the end of a file.
pub const SEEK_END: usize = contract::SEEK_END as usize;
/// File status type for a regular file.
pub const FILE_TYPE_REGULAR: u64 = contract::FILE_TYPE_REGULAR;
/// File status type for a directory.
pub const FILE_TYPE_DIRECTORY: u64 = contract::FILE_TYPE_DIRECTORY;
/// File status type for a device.
pub const FILE_TYPE_DEVICE: u64 = contract::FILE_TYPE_DEVICE;
/// Linux-compatible not found error as a signed syscall result.
pub const ERROR_NOT_FOUND: isize = contract::ERROR_NOT_FOUND;
/// Linux-compatible bad file descriptor error as a signed syscall result.
pub const ERROR_BAD_FILE_DESCRIPTOR: isize = contract::ERROR_BAD_FILE_DESCRIPTOR;
/// Bad address error return value as a signed syscall result.
pub const ERROR_BAD_ADDRESS: isize = contract::ERROR_BAD_ADDRESS;
/// Linux-compatible not implemented error as a signed syscall result.
pub const ERROR_NOT_IMPLEMENTED: isize = contract::ERROR_NOT_IMPLEMENTED;

/// Write `buffer` to an open file descriptor.
#[inline(always)]
pub fn write(file_descriptor: usize, buffer: &[u8]) -> isize {
    syscall3(
        SYS_WRITE,
        file_descriptor,
        buffer.as_ptr() as usize,
        buffer.len(),
    )
}

/// Read bytes from an open file descriptor into `buffer`.
#[inline(always)]
pub fn read(file_descriptor: usize, buffer: &mut [u8]) -> isize {
    syscall3(
        SYS_READ,
        file_descriptor,
        buffer.as_mut_ptr() as usize,
        buffer.len(),
    )
}

/// Open a null-terminated path as read-only.
#[inline(always)]
pub fn open(path: &[u8]) -> isize {
    open_with_options(path, OPEN_READ_ONLY, 0)
}

/// Open a null-terminated path with Linux-like flags and mode arguments.
#[inline(always)]
pub fn open_with_options(path: &[u8], flags: usize, mode: usize) -> isize {
    syscall3(SYS_OPEN, path.as_ptr() as usize, flags, mode)
}

/// Open a null-terminated path relative to a directory descriptor.
#[inline(always)]
pub fn openat(directory_file_descriptor: usize, path: &[u8], flags: usize, mode: usize) -> isize {
    syscall4(
        SYS_OPENAT,
        directory_file_descriptor,
        path.as_ptr() as usize,
        flags,
        mode,
    )
}

/// Close an open file descriptor.
#[inline(always)]
pub fn close(file_descriptor: usize) -> isize {
    syscall1(SYS_CLOSE, file_descriptor)
}

/// Read metadata for an open file descriptor.
#[inline(always)]
pub fn fstat(file_descriptor: usize, stat: &mut FileStat) -> isize {
    syscall2(SYS_FSTAT, file_descriptor, stat as *mut FileStat as usize)
}

/// Read directory entries from an open directory descriptor.
#[inline(always)]
pub fn getdents64(file_descriptor: usize, entries: &mut [UserDirectoryEntry]) -> isize {
    syscall3(
        SYS_GETDENTS64,
        file_descriptor,
        entries.as_mut_ptr() as usize,
        core::mem::size_of_val(entries),
    )
}

/// Seek an open file descriptor and return the new offset.
#[inline(always)]
pub fn lseek(file_descriptor: usize, offset: isize, whence: usize) -> isize {
    syscall3(SYS_LSEEK, file_descriptor, offset as usize, whence)
}

/// Move or query the user heap break.
#[inline(always)]
pub fn brk(requested_break: usize) -> isize {
    syscall1(SYS_BRK, requested_break)
}

/// Return the current ManaOS task identifier.
#[inline(always)]
pub fn getpid() -> isize {
    syscall1(SYS_GETPID, 0)
}

/// Terminate the current user task.
#[inline(always)]
pub fn exit(code: usize) -> ! {
    let _ = syscall1(SYS_EXIT, code);

    loop {
        core::hint::spin_loop();
    }
}

/// Terminate all threads in the current user task.
#[inline(always)]
pub fn exit_group(code: usize) -> ! {
    let _ = syscall1(SYS_EXIT_GROUP, code);

    loop {
        core::hint::spin_loop();
    }
}
