//! # `mana_userland::syscall`
//!
//! ## Owns
//! - Syscall contract module wiring
//! - Public syscall wrapper and raw syscall re-exports
//!
//! ## Does NOT own
//! - Kernel syscall dispatch
//! - File descriptor lifetime policy
//! - Userland command parsing
//! - Raw inline assembly instruction wrappers (-> `raw`)
//! - Safe-ish syscall wrapper implementation (-> `api`)
//!
//! ## Public API
//! - [`read`] - Read bytes from an open file descriptor
//! - [`write`] - Write bytes to an open file descriptor
//! - [`open`] - Open a null-terminated path
//! - [`fstat`] - Read metadata for an open file descriptor
//! - [`lseek`] - Seek an open file descriptor
//! - [`brk`] - Move or query the user heap break
//! - [`nanosleep`] - Sleep until the requested duration elapses
//! - [`mmap`] - Map private user memory
//! - [`mmap_anonymous`] - Map anonymous private user memory
//! - [`munmap`] - Unmap private user memory
//! - [`getpid`] - Return the current task identifier
//! - [`getppid`] - Return the parent task identifier
//! - [`execve`] - Replace the current process image
//! - [`exit`] - Terminate the current user task

mod api;
#[path = "../../../src/shared/syscall_contract.rs"]
mod contract;
mod raw;

pub use api::{
    brk, close, execve, exit, exit_group, fstat, getdents64, getpid, getppid, lseek, mmap,
    mmap_anonymous, mmap_file_private, munmap, nanosleep, open, open_with_options, openat, read,
    write, AT_FDCWD, ERROR_ARGUMENT_LIST_TOO_LONG, ERROR_BAD_ADDRESS, ERROR_BAD_FILE_DESCRIPTOR,
    ERROR_FILE_EXISTS, ERROR_INVALID_ARGUMENT, ERROR_NOT_FOUND, ERROR_NOT_IMPLEMENTED,
    FILE_TYPE_DEVICE, FILE_TYPE_DIRECTORY, FILE_TYPE_REGULAR, MAP_ANONYMOUS, MAP_FIXED,
    MAP_FIXED_NOREPLACE, MAP_PRIVATE, OPEN_READ_ONLY, PROT_EXEC, PROT_READ, PROT_WRITE, SEEK_CUR,
    SEEK_END, SEEK_SET, SYS_BRK, SYS_CLOSE, SYS_EXECVE, SYS_EXIT, SYS_EXIT_GROUP, SYS_FSTAT,
    SYS_GETDENTS64, SYS_GETPID, SYS_GETPPID, SYS_LSEEK, SYS_MMAP, SYS_MUNMAP, SYS_NANOSLEEP,
    SYS_OPEN, SYS_OPENAT, SYS_READ, SYS_WRITE,
};
pub use contract::{
    UserDirectoryEntry, UserFileStat as FileStat, UserTimespec as Timespec,
    DIRECTORY_ENTRY_NAME_BYTES,
};
pub use raw::{syscall1, syscall2, syscall3, syscall4, syscall5, syscall6};
