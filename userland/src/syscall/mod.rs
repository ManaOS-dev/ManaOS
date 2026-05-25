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
//! - [`exit`] - Terminate the current user task

mod raw;

pub use raw::{syscall1, syscall2, syscall3, syscall4};

/// Linux-compatible read syscall number.
pub const SYS_READ: usize = 0;
/// Linux-compatible write syscall number.
pub const SYS_WRITE: usize = 1;
/// Linux-compatible open syscall number.
pub const SYS_OPEN: usize = 2;
/// Linux-compatible close syscall number.
pub const SYS_CLOSE: usize = 3;
/// Linux-compatible get-process-identifier syscall number.
pub const SYS_GETPID: usize = 39;
/// Linux-compatible exit syscall number.
pub const SYS_EXIT: usize = 60;
/// Linux-compatible exit-group syscall number.
pub const SYS_EXIT_GROUP: usize = 231;
/// Linux-compatible open-at syscall number.
pub const SYS_OPENAT: usize = 257;

/// File opened for read-only access.
pub const OPEN_READ_ONLY: usize = 0;
/// Linux-compatible current-working-directory marker for `openat`.
pub const AT_FDCWD: usize = usize::MAX - 99;
/// Linux-compatible not found error as a signed syscall result.
pub const ERROR_NOT_FOUND: isize = -2;
/// Linux-compatible bad file descriptor error as a signed syscall result.
pub const ERROR_BAD_FILE_DESCRIPTOR: isize = -9;
/// Bad address error return value as a signed syscall result.
pub const ERROR_BAD_ADDRESS: isize = -14;
/// Linux-compatible not implemented error as a signed syscall result.
pub const ERROR_NOT_IMPLEMENTED: isize = -38;

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
