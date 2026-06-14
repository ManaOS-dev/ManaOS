//! Safe-ish no_std syscall wrapper functions and constants.

use super::contract;
use super::contract::{UserDirectoryEntry, UserFileStat as FileStat, UserTimespec as Timespec};
use super::raw::{syscall1, syscall2, syscall3, syscall4, syscall6};

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
/// Linux-compatible memory-map syscall number.
pub const SYS_MMAP: usize = contract::SYS_MMAP as usize;
/// Linux-compatible memory-unmap syscall number.
pub const SYS_MUNMAP: usize = contract::SYS_MUNMAP as usize;
/// Linux-compatible heap break syscall number.
pub const SYS_BRK: usize = contract::SYS_BRK as usize;
/// Linux-compatible high-resolution sleep syscall number.
pub const SYS_NANOSLEEP: usize = contract::SYS_NANOSLEEP as usize;
/// Linux-compatible get-process-identifier syscall number.
pub const SYS_GETPID: usize = contract::SYS_GETPID as usize;
/// Linux-compatible execute-program syscall number.
pub const SYS_EXECVE: usize = contract::SYS_EXECVE as usize;
/// Linux-compatible exit syscall number.
pub const SYS_EXIT: usize = contract::SYS_EXIT as usize;
/// Linux-compatible wait4 syscall number reserved for the `ManaOS` `waitpid` subset.
pub const SYS_WAITPID: usize = contract::SYS_WAITPID as usize;
/// Linux-compatible get-parent-process-identifier syscall number.
pub const SYS_GETPPID: usize = contract::SYS_GETPPID as usize;
/// Linux-compatible get-directory-entries syscall number.
pub const SYS_GETDENTS64: usize = contract::SYS_GETDENTS64 as usize;
/// Linux-compatible exit-group syscall number.
pub const SYS_EXIT_GROUP: usize = contract::SYS_EXIT_GROUP as usize;
/// Linux-compatible open-at syscall number.
pub const SYS_OPENAT: usize = contract::SYS_OPENAT as usize;

/// File opened for read-only access.
pub const OPEN_READ_ONLY: usize = contract::OPEN_READ_ONLY as usize;
/// Linux-compatible close-on-exec open flag.
pub const OPEN_CLOSE_ON_EXEC: usize = contract::OPEN_CLOSE_ON_EXEC as usize;
/// Linux-compatible current-working-directory marker for `openat`.
pub const AT_FDCWD: usize = contract::AT_FDCWD as usize;
/// Seek relative to the start of a file.
pub const SEEK_SET: usize = contract::SEEK_SET as usize;
/// Seek relative to the current file offset.
pub const SEEK_CUR: usize = contract::SEEK_CUR as usize;
/// Seek relative to the end of a file.
pub const SEEK_END: usize = contract::SEEK_END as usize;
/// Match any child process in `waitpid`.
pub const WAIT_ANY: isize = contract::WAIT_ANY;
/// Return immediately from `waitpid` if no matching child has exited.
pub const WNOHANG: usize = contract::WNOHANG as usize;
/// Mapping pages may be read by user code.
pub const PROT_READ: usize = contract::PROT_READ as usize;
/// Mapping pages may be written by user code.
pub const PROT_WRITE: usize = contract::PROT_WRITE as usize;
/// Mapping pages may be executed by user code.
pub const PROT_EXEC: usize = contract::PROT_EXEC as usize;
/// Mapping is private to the current process.
pub const MAP_PRIVATE: usize = contract::MAP_PRIVATE as usize;
/// Fixed mapping replaces overlapping mappings at the requested address.
pub const MAP_FIXED: usize = contract::MAP_FIXED as usize;
/// Mapping is anonymous and not backed by a file descriptor.
pub const MAP_ANONYMOUS: usize = contract::MAP_ANONYMOUS as usize;
/// Fixed mapping must fail when the requested range is already mapped.
pub const MAP_FIXED_NOREPLACE: usize = contract::MAP_FIXED_NOREPLACE as usize;
/// File status type for a regular file.
pub const FILE_TYPE_REGULAR: u64 = contract::FILE_TYPE_REGULAR;
/// File status type for a directory.
pub const FILE_TYPE_DIRECTORY: u64 = contract::FILE_TYPE_DIRECTORY;
/// File status type for a device.
pub const FILE_TYPE_DEVICE: u64 = contract::FILE_TYPE_DEVICE;
/// Linux-compatible not found error as a signed syscall result.
pub const ERROR_NOT_FOUND: isize = contract::ERROR_NOT_FOUND;
/// Linux-compatible argument-list-too-long error as a signed syscall result.
pub const ERROR_ARGUMENT_LIST_TOO_LONG: isize = contract::ERROR_ARGUMENT_LIST_TOO_LONG;
/// Linux-compatible bad file descriptor error as a signed syscall result.
pub const ERROR_BAD_FILE_DESCRIPTOR: isize = contract::ERROR_BAD_FILE_DESCRIPTOR;
/// Linux-compatible no-child-process error as a signed syscall result.
pub const ERROR_NO_CHILD: isize = contract::ERROR_NO_CHILD;
/// Bad address error return value as a signed syscall result.
pub const ERROR_BAD_ADDRESS: isize = contract::ERROR_BAD_ADDRESS;
/// Linux-compatible file exists error as a signed syscall result.
pub const ERROR_FILE_EXISTS: isize = contract::ERROR_FILE_EXISTS;
/// Linux-compatible is-directory error as a signed syscall result.
pub const ERROR_IS_DIRECTORY: isize = contract::ERROR_IS_DIRECTORY;
/// Linux-compatible invalid argument error as a signed syscall result.
pub const ERROR_INVALID_ARGUMENT: isize = contract::ERROR_INVALID_ARGUMENT;
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

/// Sleep until `duration` elapses.
#[inline(always)]
pub fn nanosleep(duration: &Timespec) -> isize {
    syscall2(SYS_NANOSLEEP, duration as *const Timespec as usize, 0)
}

/// Map private memory in the current ManaOS task.
#[inline(always)]
pub fn mmap(
    address: usize,
    length: usize,
    protection: usize,
    flags: usize,
    file_descriptor: usize,
    offset: usize,
) -> isize {
    syscall6(
        SYS_MMAP,
        address,
        length,
        protection,
        flags,
        file_descriptor,
        offset,
    )
}

/// Map anonymous private memory in the current ManaOS task.
#[inline(always)]
pub fn mmap_anonymous(address: usize, length: usize, protection: usize, flags: usize) -> isize {
    mmap(
        address,
        length,
        protection,
        flags | MAP_ANONYMOUS,
        usize::MAX,
        0,
    )
}

/// Map a private copy of file bytes in the current ManaOS task.
#[inline(always)]
pub fn mmap_file_private(
    address: usize,
    length: usize,
    protection: usize,
    flags: usize,
    file_descriptor: usize,
    offset: usize,
) -> isize {
    mmap(
        address,
        length,
        protection,
        flags & !MAP_ANONYMOUS,
        file_descriptor,
        offset,
    )
}

/// Unmap a private mapping previously returned by [`mmap`].
#[inline(always)]
pub fn munmap(address: usize, length: usize) -> isize {
    syscall2(SYS_MUNMAP, address, length)
}

/// Return the current ManaOS task identifier.
#[inline(always)]
pub fn getpid() -> isize {
    syscall1(SYS_GETPID, 0)
}

/// Return the parent ManaOS task identifier.
#[inline(always)]
pub fn getppid() -> isize {
    syscall1(SYS_GETPPID, 0)
}

/// Wait for a child process and optionally store its wait status.
#[inline(always)]
pub fn waitpid(process_identifier: isize, status_pointer: *mut i32, options: usize) -> isize {
    syscall3(
        SYS_WAITPID,
        process_identifier as usize,
        status_pointer as usize,
        options,
    )
}

/// Replace the current process image with a new executable.
///
/// `path` must point to a NUL-terminated path. `arguments` and `environment`
/// must point to NUL-terminated pointer arrays, or be null for an empty vector.
#[inline(always)]
pub fn execve(path: &[u8], arguments: *const *const u8, environment: *const *const u8) -> isize {
    syscall3(
        SYS_EXECVE,
        path.as_ptr() as usize,
        arguments as usize,
        environment as usize,
    )
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
