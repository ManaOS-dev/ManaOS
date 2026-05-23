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
//! - [`SYS_WRITE`] - Write syscall number
//! - [`SYS_EXIT`] - Exit syscall number

const ERROR_BAD_FILE_DESCRIPTOR: u64 = u64::MAX - 8;
const ERROR_BAD_ADDRESS: u64 = u64::MAX - 13;
const ERROR_NOT_IMPLEMENTED: u64 = u64::MAX - 37;
const USER_SPACE_END: usize = 0x0000_8000_0000_0000;

/// Write syscall number.
pub const SYS_WRITE: u64 = 1;
/// Exit syscall number.
pub const SYS_EXIT: u64 = 2;
/// Internal sentinel telling the syscall entry code to return to the kernel.
pub const USER_EXIT_SENTINEL: u64 = u64::MAX;

/// Dispatch one syscall using the `ManaOS` syscall ABI.
///
/// The ABI is:
/// - `rax`: syscall number
/// - `rdi`: first argument
/// - `rsi`: second argument
/// - `rdx`: third argument
#[no_mangle]
pub extern "C" fn syscall_dispatch(
    syscall_number: u64,
    first_argument: u64,
    second_argument: u64,
    third_argument: u64,
) -> u64 {
    match syscall_number {
        SYS_WRITE => sys_write(first_argument, second_argument, third_argument),
        SYS_EXIT => sys_exit(first_argument),
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

fn sys_exit(exit_code: u64) -> u64 {
    if let Some(task_id) = crate::kernel::task::finish_current_task(exit_code) {
        crate::serial_println!(
            "[ok   ] User task exited: code={} task={}",
            exit_code,
            task_id
        );
    } else {
        crate::serial_println!("[warn ] SYS_EXIT called without a running task");
    }

    USER_EXIT_SENTINEL
}

fn copy_from_user(user_pointer: usize, length: usize) -> Option<&'static [u8]> {
    if length == 0 {
        return Some(&[]);
    }

    let end = user_pointer.checked_add(length)?;
    if user_pointer == 0 || end > USER_SPACE_END {
        return None;
    }

    // SAFETY: This is the Phase 5B-1 bootstrap validator. It only accepts
    // canonical lower-half user addresses and relies on current shared page
    // tables to make mapped user memory readable by the kernel.
    Some(unsafe { core::slice::from_raw_parts(user_pointer as *const u8, length) })
}
