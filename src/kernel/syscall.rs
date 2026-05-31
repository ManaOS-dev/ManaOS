//! # `kernel::syscall`
//!
//! ## Owns
//! - Kernel syscall number definitions
//! - Syscall argument dispatch
//! - Minimal user pointer copying for early userland I/O
//!
//! ## Does NOT own
//! - Architecture-specific `SYSCALL`/`SYSRET` register entry
//! - Full user pointer validation
//! - Per-process file descriptor tables
//!
//! ## Public API
//! - [`dispatch`] - Dispatch one syscall from architecture entry code
//! - [`SYS_READ`] - Linux-compatible read syscall number
//! - [`SYS_WRITE`] - Linux-compatible write syscall number
//! - [`SYS_OPEN`] - Linux-compatible open syscall number
//! - [`SYS_CLOSE`] - Linux-compatible close syscall number
//! - [`SYS_FSTAT`] - Linux-compatible file status syscall number
//! - [`SYS_LSEEK`] - Linux-compatible seek syscall number
//! - [`SYS_EXIT`] - Linux-compatible exit syscall number
//! - [`SYS_EXIT_GROUP`] - Linux-compatible process exit syscall number
//! - [`SYS_GETPID`] - Linux-compatible get-process-identifier syscall number
//! - [`SYS_OPENAT`] - Linux-compatible open-at syscall number

use alloc::string::String;

const ERROR_NOT_FOUND: u64 = linux_error(2);
const ERROR_BAD_FILE_DESCRIPTOR: u64 = linux_error(9);
const ERROR_BAD_ADDRESS: u64 = linux_error(14);
const ERROR_NOT_DIRECTORY: u64 = linux_error(20);
const ERROR_IS_DIRECTORY: u64 = linux_error(21);
const ERROR_INVALID_ARGUMENT: u64 = linux_error(22);
const ERROR_TOO_MANY_OPEN_FILES: u64 = linux_error(24);
const ERROR_NOT_IMPLEMENTED: u64 = linux_error(38);
const ERROR_NOT_SUPPORTED: u64 = linux_error(95);
const AT_FDCWD: u64 = u64::MAX - 99;
const SEEK_SET: u64 = 0;
const SEEK_CUR: u64 = 1;
const SEEK_END: u64 = 2;
const USER_FILE_STAT_BYTES: usize = 24;
const USER_FILE_TYPE_REGULAR: u64 = 1;
const USER_FILE_TYPE_DIRECTORY: u64 = 2;
const USER_FILE_TYPE_DEVICE: u64 = 3;
const MAX_USER_STRING_LENGTH: usize = 256;
const USER_SPACE_END: usize = 0x0000_8000_0000_0000;

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
/// Linux-compatible exit syscall number.
pub const SYS_EXIT: u64 = 60;
/// Linux-compatible get-process-identifier syscall number.
pub const SYS_GETPID: u64 = 39;
/// Linux-compatible exit-group syscall number.
pub const SYS_EXIT_GROUP: u64 = 231;
/// Linux-compatible open-at syscall number.
pub const SYS_OPENAT: u64 = 257;
/// Internal sentinel telling the syscall entry code to return to the kernel.
pub const USER_EXIT_SENTINEL: u64 = u64::MAX;

const fn linux_error(errno: u64) -> u64 {
    0_u64.wrapping_sub(errno)
}

/// Dispatch one syscall using the `ManaOS` syscall ABI.
///
/// The ABI is:
/// - `rax`: syscall number
/// - `rdi`: first argument
/// - `rsi`: second argument
/// - `rdx`: third argument
/// - `r10`: fourth argument
#[no_mangle]
pub extern "C" fn syscall_dispatch(
    syscall_number: u64,
    first_argument: u64,
    second_argument: u64,
    third_argument: u64,
    fourth_argument: u64,
) -> u64 {
    match syscall_number {
        SYS_WRITE => sys_write(first_argument, second_argument, third_argument),
        SYS_EXIT | SYS_EXIT_GROUP => sys_exit(first_argument),
        SYS_OPEN => sys_open(first_argument, second_argument, third_argument),
        SYS_OPENAT => sys_openat(
            first_argument,
            second_argument,
            third_argument,
            fourth_argument,
        ),
        SYS_CLOSE => sys_close(first_argument),
        SYS_FSTAT => sys_fstat(first_argument, second_argument),
        SYS_LSEEK => sys_lseek(first_argument, second_argument, third_argument),
        SYS_READ => sys_read(first_argument, second_argument, third_argument),
        SYS_GETPID => sys_getpid(),
        _ => ERROR_NOT_IMPLEMENTED,
    }
}

fn sys_write(file_descriptor: u64, user_pointer: u64, length: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };
    let Ok(user_pointer) = usize::try_from(user_pointer) else {
        return ERROR_BAD_ADDRESS;
    };
    let Ok(length) = usize::try_from(length) else {
        return ERROR_BAD_ADDRESS;
    };

    let Some(buffer) = copy_from_user(user_pointer, length) else {
        return ERROR_BAD_ADDRESS;
    };

    match crate::kernel::filesystem::write(file_descriptor, buffer) {
        Ok(bytes_written) => u64::try_from(bytes_written).unwrap_or(u64::MAX),
        Err(error) => filesystem_error_to_linux(error),
    }
}

fn sys_open(user_path_pointer: u64, flags: u64, mode: u64) -> u64 {
    crate::log_debug!(
        "syscall",
        "open(path={:#018x}, flags={:#x}, mode={:#o})",
        user_path_pointer,
        flags,
        mode
    );

    let Ok(user_path_pointer) = usize::try_from(user_path_pointer) else {
        return ERROR_BAD_ADDRESS;
    };

    let Some(path) = copy_cstr_from_user(user_path_pointer) else {
        return ERROR_BAD_ADDRESS;
    };

    match crate::kernel::filesystem::open(&path) {
        Ok(file_descriptor) => u64::try_from(file_descriptor).unwrap_or(u64::MAX),
        Err(error) => filesystem_error_to_linux(error),
    }
}

fn sys_openat(
    directory_file_descriptor: u64,
    user_path_pointer: u64,
    flags: u64,
    mode: u64,
) -> u64 {
    if directory_file_descriptor != AT_FDCWD {
        return ERROR_NOT_IMPLEMENTED;
    }

    sys_open(user_path_pointer, flags, mode)
}

fn sys_close(file_descriptor: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };

    match crate::kernel::filesystem::close(file_descriptor) {
        Ok(()) => 0,
        Err(error) => filesystem_error_to_linux(error),
    }
}

fn sys_fstat(file_descriptor: u64, user_stat_pointer: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };
    let Ok(user_stat_pointer) = usize::try_from(user_stat_pointer) else {
        return ERROR_BAD_ADDRESS;
    };

    let Some(buffer) = copy_to_user(user_stat_pointer, USER_FILE_STAT_BYTES) else {
        return ERROR_BAD_ADDRESS;
    };

    match crate::kernel::filesystem::descriptor_metadata(file_descriptor) {
        Ok(metadata) => {
            write_user_file_stat(buffer, metadata);
            crate::log_info!(
                "syscall",
                "fstat -> fd={} type={:?} size={} writable={}",
                file_descriptor,
                metadata.file_type,
                metadata.size,
                metadata.writable
            );
            0
        }
        Err(error) => filesystem_error_to_linux(error),
    }
}

fn sys_read(file_descriptor: u64, user_pointer: u64, length: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };
    let Ok(user_pointer) = usize::try_from(user_pointer) else {
        return ERROR_BAD_ADDRESS;
    };
    let Ok(length) = usize::try_from(length) else {
        return ERROR_BAD_ADDRESS;
    };

    let Some(buffer) = copy_to_user(user_pointer, length) else {
        return ERROR_BAD_ADDRESS;
    };

    match crate::kernel::filesystem::read(file_descriptor, buffer) {
        Ok(bytes_read) => u64::try_from(bytes_read).unwrap_or(u64::MAX),
        Err(error) => filesystem_error_to_linux(error),
    }
}

fn sys_lseek(file_descriptor: u64, offset: u64, whence: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };
    let offset = i64::from_ne_bytes(offset.to_ne_bytes());
    let whence_argument = whence;
    let whence = match whence_argument {
        SEEK_SET => crate::kernel::filesystem::SeekWhence::Start,
        SEEK_CUR => crate::kernel::filesystem::SeekWhence::Current,
        SEEK_END => crate::kernel::filesystem::SeekWhence::End,
        _ => return ERROR_INVALID_ARGUMENT,
    };

    match crate::kernel::filesystem::seek_from(file_descriptor, offset, whence) {
        Ok(next_offset) => {
            crate::log_info!(
                "syscall",
                "lseek -> fd={} offset={} whence={} next={}",
                file_descriptor,
                offset,
                whence_argument,
                next_offset
            );
            u64::try_from(next_offset).unwrap_or(u64::MAX)
        }
        Err(error) => filesystem_error_to_linux(error),
    }
}

fn write_user_file_stat(buffer: &mut [u8], metadata: crate::kernel::filesystem::FileMetadata) {
    let file_type = match metadata.file_type {
        crate::kernel::filesystem::FileType::Regular => USER_FILE_TYPE_REGULAR,
        crate::kernel::filesystem::FileType::Directory => USER_FILE_TYPE_DIRECTORY,
        crate::kernel::filesystem::FileType::Device => USER_FILE_TYPE_DEVICE,
    };
    write_user_u64(buffer, 0, file_type);
    write_user_u64(
        buffer,
        8,
        u64::try_from(metadata.size).expect("filesystem metadata size must fit in u64"),
    );
    write_user_u64(buffer, 16, u64::from(metadata.writable));
}

fn write_user_u64(buffer: &mut [u8], offset: usize, value: u64) {
    buffer[offset..offset + core::mem::size_of::<u64>()].copy_from_slice(&value.to_ne_bytes());
}

fn sys_exit(exit_code: u64) -> u64 {
    if let Some(task_id) = crate::kernel::task::finish_current_task(exit_code) {
        crate::log_info!(
            "syscall",
            "User task exited: code={} task={}",
            exit_code,
            task_id
        );
    } else {
        crate::log_warn!("syscall", "SYS_EXIT called without a running task");
    }

    USER_EXIT_SENTINEL
}

fn sys_getpid() -> u64 {
    match crate::kernel::task::get_current_task_id() {
        Some(task_id) => {
            crate::log_info!("syscall", "getpid -> task={}", task_id);
            task_id
        }
        None => ERROR_NOT_IMPLEMENTED,
    }
}

fn filesystem_error_to_linux(error: crate::kernel::filesystem::FileSystemError) -> u64 {
    match error {
        crate::kernel::filesystem::FileSystemError::NotFound => ERROR_NOT_FOUND,
        crate::kernel::filesystem::FileSystemError::InvalidFileDescriptor => {
            ERROR_BAD_FILE_DESCRIPTOR
        }
        crate::kernel::filesystem::FileSystemError::UnsupportedOperation => ERROR_NOT_SUPPORTED,
        crate::kernel::filesystem::FileSystemError::TooManyOpenFiles => ERROR_TOO_MANY_OPEN_FILES,
        crate::kernel::filesystem::FileSystemError::AlreadyInitialized
        | crate::kernel::filesystem::FileSystemError::InvalidPath
        | crate::kernel::filesystem::FileSystemError::InvalidArgument => ERROR_INVALID_ARGUMENT,
        crate::kernel::filesystem::FileSystemError::NotDirectory => ERROR_NOT_DIRECTORY,
        crate::kernel::filesystem::FileSystemError::IsDirectory => ERROR_IS_DIRECTORY,
    }
}

fn validate_user_range(user_pointer: usize, length: usize) -> Option<()> {
    if length == 0 {
        return Some(());
    }

    let end = user_pointer.checked_add(length)?;
    if user_pointer == 0 || end > USER_SPACE_END {
        return None;
    }

    Some(())
}

fn copy_from_user(user_pointer: usize, length: usize) -> Option<&'static [u8]> {
    if length == 0 {
        return Some(&[]);
    }

    validate_user_range(user_pointer, length)?;
    if !crate::kernel::memory::paging::is_user_range_mapped_readable(user_pointer, length) {
        return None;
    }

    // SAFETY: The range has been bounds-checked and page-table validated as
    // present user-accessible memory before creating the kernel slice.
    Some(unsafe { core::slice::from_raw_parts(user_pointer as *const u8, length) })
}

fn copy_to_user(user_pointer: usize, length: usize) -> Option<&'static mut [u8]> {
    if length == 0 {
        return Some(&mut []);
    }

    validate_user_range(user_pointer, length)?;
    if !crate::kernel::memory::paging::is_user_range_mapped_writable(user_pointer, length) {
        return None;
    }

    // SAFETY: The range has been bounds-checked and page-table validated as
    // present writable user-accessible memory before creating the kernel slice.
    Some(unsafe { core::slice::from_raw_parts_mut(user_pointer as *mut u8, length) })
}

fn copy_cstr_from_user(user_pointer: usize) -> Option<String> {
    let bytes = copy_from_user(user_pointer, MAX_USER_STRING_LENGTH)?;

    let mut path = String::new();
    for byte in bytes {
        if *byte == 0 {
            return Some(path);
        }

        path.push(char::from(*byte));
    }

    None
}
