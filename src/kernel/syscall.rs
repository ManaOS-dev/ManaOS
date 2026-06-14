//! # `kernel::syscall`
//!
//! ## Owns
//! - Kernel syscall number definitions
//! - Syscall argument dispatch
//! - Mapping kernel filesystem errors to Linux-like syscall results
//! - Optional syscall tracing diagnostics
//!
//! ## Does NOT own
//! - Architecture-specific `SYSCALL`/`SYSRET` register entry
//! - User pointer validation (-> `kernel::memory::user_pointer`)
//! - Per-process file descriptor tables
//!
//! ## Public API
//! - [`syscall_dispatch`] - Dispatch one syscall from architecture entry code
//! - [`syscall_dispatch_from_trap_frame`] - Dispatch one syscall from a saved user frame
//! - [`set_trace_enabled`] - Enable or disable syscall trace logging
//! - [`reset_trace`] - Clear syscall trace accounting
//! - [`get_trace_diagnostics`] - Read syscall trace diagnostics
//! - [`SYS_READ`] - Linux-compatible read syscall number
//! - [`SYS_WRITE`] - Linux-compatible write syscall number
//! - [`SYS_OPEN`] - Linux-compatible open syscall number
//! - [`SYS_CLOSE`] - Linux-compatible close syscall number
//! - [`SYS_FSTAT`] - Linux-compatible file status syscall number
//! - [`SYS_LSEEK`] - Linux-compatible seek syscall number
//! - [`SYS_MMAP`] - Linux-compatible memory-map syscall number
//! - [`SYS_MUNMAP`] - Linux-compatible memory-unmap syscall number
//! - [`SYS_BRK`] - Linux-compatible heap break syscall number
//! - [`SYS_NANOSLEEP`] - Linux-compatible high-resolution sleep syscall number
//! - [`SYS_EXIT`] - Linux-compatible exit syscall number
//! - [`SYS_GETDENTS64`] - Linux-compatible get-directory-entries syscall number
//! - [`SYS_EXIT_GROUP`] - Linux-compatible process exit syscall number
//! - [`SYS_GETPID`] - Linux-compatible get-process-identifier syscall number
//! - [`SYS_GETPPID`] - Linux-compatible get-parent-process-identifier syscall number
//! - [`SYS_OPENAT`] - Linux-compatible open-at syscall number

use crate::kernel::memory::{
    address::{UserCString, UserReadableRange, UserVirtualRange, UserWritableRange},
    user_pointer,
};
use crate::kernel::task::context::UserTrapFrame;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[allow(dead_code)]
#[path = "../shared/syscall_contract.rs"]
mod contract;
mod memory;

pub use contract::{
    SYS_BRK, SYS_CLOSE, SYS_EXIT, SYS_EXIT_GROUP, SYS_FSTAT, SYS_GETDENTS64, SYS_GETPID,
    SYS_GETPPID, SYS_LSEEK, SYS_MMAP, SYS_MUNMAP, SYS_NANOSLEEP, SYS_OPEN, SYS_OPENAT, SYS_READ,
    SYS_WRITE,
};

const ERROR_NOT_FOUND: u64 = linux_error(2);
const ERROR_BAD_FILE_DESCRIPTOR: u64 = linux_error(9);
const ERROR_OUT_OF_MEMORY: u64 = linux_error(12);
const ERROR_BAD_ADDRESS: u64 = linux_error(14);
const ERROR_FILE_EXISTS: u64 = linux_error(17);
const ERROR_NOT_DIRECTORY: u64 = linux_error(20);
const ERROR_IS_DIRECTORY: u64 = linux_error(21);
const ERROR_INVALID_ARGUMENT: u64 = linux_error(22);
const ERROR_TOO_MANY_OPEN_FILES: u64 = linux_error(24);
const ERROR_NOT_IMPLEMENTED: u64 = linux_error(38);
const ERROR_NOT_SUPPORTED: u64 = linux_error(95);
const USER_FILE_STAT_BYTES: usize = core::mem::size_of::<contract::UserFileStat>();
const USER_DIRECTORY_ENTRY_BYTES: usize = core::mem::size_of::<contract::UserDirectoryEntry>();
const USER_TIMESPEC_BYTES: usize = core::mem::size_of::<contract::UserTimespec>();
const MAX_USER_STRING_LENGTH: usize = 256;
const PAGE_SIZE: u64 = 4096;
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
const NANOSECONDS_PER_TIMER_TICK: u64 =
    NANOSECONDS_PER_SECOND / crate::shared::TIMER_TICKS_PER_SECOND;
const LINUX_ERRNO_MAX: u64 = 4095;
/// Internal sentinel telling the syscall entry code to return to the kernel.
pub const USER_EXIT_SENTINEL: u64 = u64::MAX - LINUX_ERRNO_MAX;
/// Internal sentinel telling the syscall entry code to block and return to the kernel.
pub const USER_BLOCK_SENTINEL: u64 = u64::MAX - LINUX_ERRNO_MAX - 1;

static TRACE_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACE_RECORD_COUNT: AtomicU64 = AtomicU64::new(0);
static TRACE_LAST_VALID: AtomicBool = AtomicBool::new(false);
static TRACE_LAST_TASK_ID: AtomicU64 = AtomicU64::new(0);
static TRACE_LAST_SYSCALL_NUMBER: AtomicU64 = AtomicU64::new(0);
static TRACE_LAST_RESULT: AtomicU64 = AtomicU64::new(0);

/// Snapshot of syscall trace state and recent trace accounting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyscallTraceDiagnostics {
    enabled: bool,
    record_count: u64,
    last_task_id: Option<u64>,
    last_syscall_number: Option<u64>,
    last_result: Option<u64>,
}

impl SyscallTraceDiagnostics {
    /// Create syscall trace diagnostics from raw accounting state.
    const fn new(
        enabled: bool,
        record_count: u64,
        last_task_id: Option<u64>,
        last_syscall_number: Option<u64>,
        last_result: Option<u64>,
    ) -> Self {
        Self {
            enabled,
            record_count,
            last_task_id,
            last_syscall_number,
            last_result,
        }
    }

    /// Return whether syscall trace logging is enabled.
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Return the number of syscall trace records emitted since the last reset.
    pub const fn record_count(self) -> u64 {
        self.record_count
    }

    /// Return the last traced task identifier, if any syscall has been traced.
    pub const fn last_task_id(self) -> Option<u64> {
        self.last_task_id
    }

    /// Return the last traced syscall number, if any syscall has been traced.
    pub const fn last_syscall_number(self) -> Option<u64> {
        self.last_syscall_number
    }

    /// Return the last traced syscall result, if any syscall has been traced.
    pub const fn last_result(self) -> Option<u64> {
        self.last_result
    }
}

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
/// - `r8`: fifth argument
/// - `r9`: sixth argument
#[no_mangle]
pub extern "C" fn syscall_dispatch(
    syscall_number: u64,
    first_argument: u64,
    second_argument: u64,
    third_argument: u64,
    fourth_argument: u64,
    fifth_argument: u64,
    sixth_argument: u64,
) -> u64 {
    let task_id = crate::kernel::task::get_current_task_id();
    let arguments = [
        first_argument,
        second_argument,
        third_argument,
        fourth_argument,
        fifth_argument,
        sixth_argument,
    ];
    let result = dispatch_syscall(syscall_number, arguments);
    record_syscall_trace(task_id, syscall_number, arguments, result);
    result
}

fn dispatch_syscall(syscall_number: u64, arguments: [u64; 6]) -> u64 {
    let [first_argument, second_argument, third_argument, fourth_argument, fifth_argument, sixth_argument] =
        arguments;
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
        SYS_MMAP => memory::sys_mmap(
            first_argument,
            second_argument,
            third_argument,
            fourth_argument,
            fifth_argument,
            sixth_argument,
        ),
        SYS_MUNMAP => memory::sys_munmap(first_argument, second_argument),
        SYS_BRK => memory::sys_brk(first_argument),
        SYS_NANOSLEEP => memory::sys_nanosleep(first_argument, second_argument),
        SYS_READ => sys_read(first_argument, second_argument, third_argument),
        SYS_GETDENTS64 => sys_getdents64(first_argument, second_argument, third_argument),
        SYS_GETPID => sys_getpid(),
        SYS_GETPPID => sys_getppid(),
        _ => ERROR_NOT_IMPLEMENTED,
    }
}

/// Enable or disable syscall trace logging.
pub fn set_trace_enabled(enabled: bool) {
    TRACE_ENABLED.store(enabled, Ordering::Release);
}

/// Clear syscall trace accounting while preserving the enabled state.
pub fn reset_trace() {
    TRACE_RECORD_COUNT.store(0, Ordering::Release);
    TRACE_LAST_TASK_ID.store(0, Ordering::Release);
    TRACE_LAST_SYSCALL_NUMBER.store(0, Ordering::Release);
    TRACE_LAST_RESULT.store(0, Ordering::Release);
    TRACE_LAST_VALID.store(false, Ordering::Release);
}

/// Return syscall trace diagnostics.
pub fn get_trace_diagnostics() -> SyscallTraceDiagnostics {
    let enabled = TRACE_ENABLED.load(Ordering::Acquire);
    let record_count = TRACE_RECORD_COUNT.load(Ordering::Acquire);
    let last_valid = TRACE_LAST_VALID.load(Ordering::Acquire);
    if !last_valid {
        return SyscallTraceDiagnostics::new(enabled, record_count, None, None, None);
    }

    SyscallTraceDiagnostics::new(
        enabled,
        record_count,
        Some(TRACE_LAST_TASK_ID.load(Ordering::Acquire)),
        Some(TRACE_LAST_SYSCALL_NUMBER.load(Ordering::Acquire)),
        Some(TRACE_LAST_RESULT.load(Ordering::Acquire)),
    )
}

fn record_syscall_trace(
    task_id: Option<u64>,
    syscall_number: u64,
    arguments: [u64; 6],
    result: u64,
) {
    if !TRACE_ENABLED.load(Ordering::Acquire) {
        return;
    }

    let task_id = task_id.unwrap_or(0);
    TRACE_LAST_TASK_ID.store(task_id, Ordering::Release);
    TRACE_LAST_SYSCALL_NUMBER.store(syscall_number, Ordering::Release);
    TRACE_LAST_RESULT.store(result, Ordering::Release);
    TRACE_LAST_VALID.store(true, Ordering::Release);
    let record_index = TRACE_RECORD_COUNT
        .fetch_add(1, Ordering::AcqRel)
        .saturating_add(1);
    crate::log_info!(
        "syscall",
        "Syscall trace: record={} task={} number={} result={:#x} arg0={:#x} arg1={:#x} arg2={:#x} arg3={:#x} arg4={:#x} arg5={:#x}",
        record_index,
        task_id,
        syscall_number,
        result,
        arguments[0],
        arguments[1],
        arguments[2],
        arguments[3],
        arguments[4],
        arguments[5]
    );
}

/// Dispatch one syscall from a captured user trap frame.
///
/// The return value is written back to `trap_frame.rax` so the same frame can be
/// used by future resume paths.
///
/// # Panics
///
/// Panics if `trap_frame` is null.
///
/// # Safety
///
/// `trap_frame` must point to writable storage containing the user register
/// state captured by the architecture syscall entry path.
#[no_mangle]
pub unsafe extern "C" fn syscall_dispatch_from_trap_frame(trap_frame: *mut UserTrapFrame) -> u64 {
    assert!(
        !trap_frame.is_null(),
        "syscall trap frame pointer must be non-null"
    );

    let trap_frame_storage_address =
        u64::try_from(trap_frame.addr()).expect("syscall trap frame pointer must fit in u64");
    // SAFETY: The architecture syscall entry passes a non-null pointer to the
    // stack-resident trap frame it just populated.
    let trap_frame = unsafe { &mut *trap_frame };
    let selectors = crate::kernel::task::user_mode::get_selectors();
    trap_frame.code_segment = u64::from(selectors.code);
    trap_frame.stack_segment = u64::from(selectors.data);

    let result = syscall_dispatch(
        trap_frame.rax,
        trap_frame.rdi,
        trap_frame.rsi,
        trap_frame.rdx,
        trap_frame.r10,
        trap_frame.r8,
        trap_frame.r9,
    );
    if result == USER_BLOCK_SENTINEL {
        trap_frame.rax = 0;
        crate::kernel::task::record_current_user_trap_frame(
            *trap_frame,
            trap_frame_storage_address,
        );
        let task_id = crate::kernel::task::block_current_user_after_syscall()
            .expect("prepared user sleep must block after saving the syscall frame");
        crate::kernel::task::close_user_return_preemption_window(task_id);
        return USER_BLOCK_SENTINEL;
    }

    trap_frame.rax = result;
    if result != USER_EXIT_SENTINEL {
        crate::kernel::task::record_current_user_trap_frame(
            *trap_frame,
            trap_frame_storage_address,
        );
    }
    result
}

fn sys_write(file_descriptor: u64, user_pointer: u64, length: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };

    let Some(buffer) = copy_input_buffer(user_pointer, length) else {
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

    let Some(path) = copy_path_argument(user_path_pointer) else {
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
    if directory_file_descriptor != contract::AT_FDCWD {
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

    let Some(buffer) = copy_output_buffer(
        user_stat_pointer,
        u64::try_from(USER_FILE_STAT_BYTES).expect("user file stat size must fit in u64"),
    ) else {
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

    let Some(buffer) = copy_output_buffer(user_pointer, length) else {
        return ERROR_BAD_ADDRESS;
    };

    match crate::kernel::filesystem::read(file_descriptor, buffer) {
        Ok(bytes_read) => u64::try_from(bytes_read).unwrap_or(u64::MAX),
        Err(error) => filesystem_error_to_linux(error),
    }
}

fn sys_getdents64(file_descriptor: u64, user_pointer: u64, length: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };
    let length_argument = length;
    let Ok(length) = usize::try_from(length) else {
        return ERROR_BAD_ADDRESS;
    };
    if length < USER_DIRECTORY_ENTRY_BYTES {
        return ERROR_INVALID_ARGUMENT;
    }

    let Some(buffer) = copy_output_buffer(user_pointer, length_argument) else {
        return ERROR_BAD_ADDRESS;
    };

    let mut bytes_written = 0;
    let mut entries_written = 0;
    while bytes_written + USER_DIRECTORY_ENTRY_BYTES <= buffer.len() {
        match crate::kernel::filesystem::read_directory(file_descriptor) {
            Ok(Some(entry)) => {
                write_user_directory_entry(
                    &mut buffer[bytes_written..bytes_written + USER_DIRECTORY_ENTRY_BYTES],
                    &entry,
                );
                bytes_written += USER_DIRECTORY_ENTRY_BYTES;
                entries_written += 1;
            }
            Ok(None) => break,
            Err(error) if bytes_written > 0 => {
                crate::log_warn!(
                    "syscall",
                    "getdents64 partial -> fd={} bytes={} error={:?}",
                    file_descriptor,
                    bytes_written,
                    error
                );
                break;
            }
            Err(error) => return filesystem_error_to_linux(error),
        }
    }

    crate::log_info!(
        "syscall",
        "getdents64 -> fd={} entries={} bytes={}",
        file_descriptor,
        entries_written,
        bytes_written
    );
    u64::try_from(bytes_written).unwrap_or(u64::MAX)
}

fn sys_lseek(file_descriptor: u64, offset: u64, whence: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };
    let offset = i64::from_ne_bytes(offset.to_ne_bytes());
    let whence_argument = whence;
    let whence = match whence_argument {
        contract::SEEK_SET => crate::kernel::filesystem::SeekWhence::Start,
        contract::SEEK_CUR => crate::kernel::filesystem::SeekWhence::Current,
        contract::SEEK_END => crate::kernel::filesystem::SeekWhence::End,
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

fn copy_input_buffer(user_pointer: u64, byte_len: u64) -> Option<&'static [u8]> {
    if byte_len == 0 {
        return Some(&[]);
    }

    let range = UserVirtualRange::from_syscall_arguments(user_pointer, byte_len)?;
    user_pointer::copy_from_user(UserReadableRange::new(range))
}

fn copy_output_buffer(user_pointer: u64, byte_len: u64) -> Option<&'static mut [u8]> {
    if byte_len == 0 {
        return Some(&mut []);
    }

    let range = UserVirtualRange::from_syscall_arguments(user_pointer, byte_len)?;
    user_pointer::copy_to_user(UserWritableRange::new(range))
}

fn copy_path_argument(user_pointer: u64) -> Option<alloc::string::String> {
    let range = UserVirtualRange::from_syscall_arguments(
        user_pointer,
        u64::try_from(MAX_USER_STRING_LENGTH).expect("max user path length must fit in u64"),
    )?;
    user_pointer::copy_cstr_from_user(UserCString::new(UserReadableRange::new(range)))
}

fn write_user_file_stat(buffer: &mut [u8], metadata: crate::kernel::filesystem::FileMetadata) {
    let file_type = match metadata.file_type {
        crate::kernel::filesystem::FileType::Regular => contract::FILE_TYPE_REGULAR,
        crate::kernel::filesystem::FileType::Directory => contract::FILE_TYPE_DIRECTORY,
        crate::kernel::filesystem::FileType::Device => contract::FILE_TYPE_DEVICE,
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

fn read_user_u64(buffer: &[u8], offset: usize) -> u64 {
    let mut value = [0_u8; core::mem::size_of::<u64>()];
    value.copy_from_slice(&buffer[offset..offset + core::mem::size_of::<u64>()]);
    u64::from_ne_bytes(value)
}

fn write_user_directory_entry(
    buffer: &mut [u8],
    entry: &crate::kernel::filesystem::DirectoryEntry,
) {
    let file_type = match entry.metadata.file_type {
        crate::kernel::filesystem::FileType::Regular => contract::FILE_TYPE_REGULAR,
        crate::kernel::filesystem::FileType::Directory => contract::FILE_TYPE_DIRECTORY,
        crate::kernel::filesystem::FileType::Device => contract::FILE_TYPE_DEVICE,
    };
    let name = entry.name.as_bytes();
    let name_length = name.len().min(contract::DIRECTORY_ENTRY_NAME_BYTES);

    write_user_u64(buffer, 0, file_type);
    write_user_u64(
        buffer,
        8,
        u64::try_from(entry.metadata.size).expect("directory entry size must fit in u64"),
    );
    write_user_u64(
        buffer,
        16,
        u64::try_from(name_length).expect("directory entry name length must fit in u64"),
    );
    buffer[24..USER_DIRECTORY_ENTRY_BYTES].fill(0);
    buffer[24..24 + name_length].copy_from_slice(&name[..name_length]);
}

fn sys_exit(exit_code: u64) -> u64 {
    if let Some(task_id) = crate::kernel::task::finish_current_task(exit_code) {
        crate::kernel::task::close_user_return_preemption_window(task_id);
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

fn sys_getppid() -> u64 {
    match crate::kernel::task::get_current_parent_task_id() {
        Some(parent_task_id) => {
            crate::log_info!("syscall", "getppid -> parent={}", parent_task_id);
            parent_task_id
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
