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
//! - [`SYS_EXIT`] - Linux-compatible exit syscall number
//! - [`SYS_EXIT_GROUP`] - Linux-compatible process exit syscall number
//! - [`SYS_GETPID`] - Linux-compatible get-process-identifier syscall number
//! - [`SYS_OPENAT`] - Linux-compatible open-at syscall number

use alloc::string::String;

const ERROR_NOT_FOUND: u64 = linux_error(2);
const ERROR_BAD_FILE_DESCRIPTOR: u64 = linux_error(9);
const ERROR_BAD_ADDRESS: u64 = linux_error(14);
const ERROR_NOT_IMPLEMENTED: u64 = linux_error(38);
const AT_FDCWD: u64 = u64::MAX - 99;
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
        Err(_) => ERROR_BAD_FILE_DESCRIPTOR,
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
        Err(crate::kernel::filesystem::FileSystemError::NotFound) => ERROR_NOT_FOUND,
        Err(_) => ERROR_BAD_FILE_DESCRIPTOR,
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
        Err(_) => ERROR_BAD_FILE_DESCRIPTOR,
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
        Err(_) => ERROR_BAD_FILE_DESCRIPTOR,
    }
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
