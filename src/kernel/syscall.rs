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
//! - [`SYS_EXECVE`] - Linux-compatible execute-program syscall number
//! - [`SYS_EXIT`] - Linux-compatible exit syscall number
//! - [`SYS_WAITPID`] - Linux-compatible wait4 syscall number reserved for `waitpid`
//! - [`SYS_GETCWD`] - Linux-compatible get-current-working-directory syscall number
//! - [`SYS_GETDENTS64`] - Linux-compatible get-directory-entries syscall number
//! - [`SYS_EXIT_GROUP`] - Linux-compatible process exit syscall number
//! - [`SYS_GETPID`] - Linux-compatible get-process-identifier syscall number
//! - [`SYS_GETPPID`] - Linux-compatible get-parent-process-identifier syscall number
//! - [`SYS_OPENAT`] - Linux-compatible open-at syscall number
//! - [`SYS_CHDIR`] - Linux-compatible change-directory syscall number
//! - [`SYS_SPAWN`] - ManaOS-specific spawn syscall number

use alloc::{string::String, vec::Vec};

use crate::kernel::memory::{
    address::{PageCount, UserCString, UserReadableRange, UserVirtualRange, UserWritableRange},
    user_pointer,
};
use crate::kernel::task::{context::UserTrapFrame, UserTrapFrameSource};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[allow(dead_code)]
#[path = "../shared/syscall_contract.rs"]
mod contract;
mod memory;

pub use contract::{
    SYS_BRK, SYS_CHDIR, SYS_CLOSE, SYS_EXECVE, SYS_EXIT, SYS_EXIT_GROUP, SYS_FSTAT, SYS_GETCWD,
    SYS_GETDENTS64, SYS_GETPID, SYS_GETPPID, SYS_LSEEK, SYS_MMAP, SYS_MUNMAP, SYS_NANOSLEEP,
    SYS_OPEN, SYS_OPENAT, SYS_READ, SYS_SPAWN, SYS_WAITPID, SYS_WRITE,
};

const ERROR_NOT_FOUND: u64 = linux_error(2);
const ERROR_ARGUMENT_LIST_TOO_LONG: u64 = linux_error(7);
const ERROR_BAD_FILE_DESCRIPTOR: u64 = linux_error(9);
const ERROR_NO_CHILD: u64 = linux_error(10);
const ERROR_TRY_AGAIN: u64 = linux_error(11);
const ERROR_OUT_OF_MEMORY: u64 = linux_error(12);
const ERROR_BAD_ADDRESS: u64 = linux_error(14);
const ERROR_FILE_EXISTS: u64 = linux_error(17);
const ERROR_NOT_DIRECTORY: u64 = linux_error(20);
const ERROR_IS_DIRECTORY: u64 = linux_error(21);
const ERROR_INVALID_ARGUMENT: u64 = linux_error(22);
const ERROR_TOO_MANY_OPEN_FILES: u64 = linux_error(24);
const ERROR_RANGE: u64 = linux_error(34);
const ERROR_NOT_IMPLEMENTED: u64 = linux_error(38);
const ERROR_NOT_SUPPORTED: u64 = linux_error(95);
const WAIT_ANY_PROCESS_IDENTIFIER: i64 = contract::WAIT_ANY as i64;
const USER_FILE_STAT_BYTES: usize = core::mem::size_of::<contract::UserFileStat>();
const USER_DIRECTORY_ENTRY_BYTES: usize = core::mem::size_of::<contract::UserDirectoryEntry>();
const USER_TIMESPEC_BYTES: usize = core::mem::size_of::<contract::UserTimespec>();
const USER_WAIT_STATUS_BYTES: u64 = core::mem::size_of::<i32>() as u64;
const USER_POINTER_BYTES_U64: u64 = core::mem::size_of::<u64>() as u64;
const MAX_USER_STRING_LENGTH: usize = 256;
const MAX_USER_ENTRY_ARGUMENT_COUNT: usize = 8;
const MAX_USER_ENTRY_ENVIRONMENT_COUNT: usize = 8;
const MAX_USER_ENTRY_COPIED_STRING_BYTES: usize = 4096;
const EXECVE_USER_STACK_PAGES: PageCount = page_count(4);
const SPAWN_USER_STACK_PAGES: PageCount = page_count(4);
const PAGE_SIZE: u64 = 4096;
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
const NANOSECONDS_PER_TIMER_TICK: u64 =
    NANOSECONDS_PER_SECOND / crate::shared::TIMER_TICKS_PER_SECOND;
const LINUX_ERRNO_MAX: u64 = 4095;
/// Internal sentinel telling the syscall entry code to return to the kernel.
pub const USER_EXIT_SENTINEL: u64 = u64::MAX - LINUX_ERRNO_MAX;
/// Internal sentinel telling the syscall entry code to block and return to the kernel.
pub const USER_BLOCK_SENTINEL: u64 = u64::MAX - LINUX_ERRNO_MAX - 1;
/// Internal sentinel telling the syscall entry code to resume in a new image.
pub const USER_EXECVE_SENTINEL: u64 = u64::MAX - LINUX_ERRNO_MAX - 2;

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
        SYS_EXECVE => sys_execve(first_argument, second_argument, third_argument, None),
        SYS_WAITPID => sys_waitpid(first_argument, second_argument, third_argument),
        SYS_READ => sys_read(first_argument, second_argument, third_argument),
        SYS_GETDENTS64 => sys_getdents64(first_argument, second_argument, third_argument),
        SYS_GETCWD => sys_getcwd(first_argument, second_argument),
        SYS_GETPID => sys_getpid(),
        SYS_GETPPID => sys_getppid(),
        SYS_CHDIR => sys_chdir(first_argument),
        SYS_SPAWN => sys_spawn(first_argument, second_argument, third_argument),
        _ => ERROR_NOT_IMPLEMENTED,
    }
}

#[derive(Clone, Copy)]
enum WaitProcessSelector {
    AnyChild,
    ChildIdentifier(u64),
}

impl WaitProcessSelector {
    fn from_syscall_argument(process_identifier: u64) -> Option<Self> {
        match i64::from_ne_bytes(process_identifier.to_ne_bytes()) {
            WAIT_ANY_PROCESS_IDENTIFIER => Some(Self::AnyChild),
            process_identifier if process_identifier > 0 => u64::try_from(process_identifier)
                .ok()
                .map(Self::ChildIdentifier),
            _ => None,
        }
    }

    const fn child_task_id(self) -> Option<u64> {
        match self {
            Self::AnyChild => None,
            Self::ChildIdentifier(child_task_id) => Some(child_task_id),
        }
    }
}

fn sys_waitpid(process_identifier: u64, status_pointer: u64, options: u64) -> u64 {
    if options & !contract::WNOHANG != 0 {
        return ERROR_INVALID_ARGUMENT;
    }

    let Some(selector) = WaitProcessSelector::from_syscall_argument(process_identifier) else {
        return ERROR_INVALID_ARGUMENT;
    };
    let Some(parent_task_id) = crate::kernel::task::get_current_task_id() else {
        return ERROR_NOT_IMPLEMENTED;
    };
    let Some(has_matching_child) =
        crate::kernel::task::current_user_task_has_child(selector.child_task_id())
    else {
        return ERROR_NOT_IMPLEMENTED;
    };
    if !has_matching_child {
        return ERROR_NO_CHILD;
    }

    let wait_status_pointer = if status_pointer == 0 {
        None
    } else {
        if !validate_output_buffer(status_pointer, USER_WAIT_STATUS_BYTES) {
            return ERROR_BAD_ADDRESS;
        }
        Some(status_pointer)
    };

    if let Some(exit) =
        crate::kernel::task::collect_waitable_child_exit(parent_task_id, selector.child_task_id())
    {
        if let Some(status_pointer) = wait_status_pointer {
            let buffer = copy_output_buffer(status_pointer, USER_WAIT_STATUS_BYTES)
                .expect("validated waitpid status pointer must remain writable");
            write_user_u32(buffer, 0, exit.wait_status());
        }
        return exit.task_id();
    }

    if options & contract::WNOHANG != 0 {
        return 0;
    }

    if crate::kernel::task::prepare_current_user_waitpid(
        selector.child_task_id(),
        wait_status_pointer,
    )
    .is_none()
    {
        return ERROR_NOT_IMPLEMENTED;
    }

    USER_BLOCK_SENTINEL
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

    let task_id = crate::kernel::task::get_current_task_id();
    let syscall_number = trap_frame.rax;
    let arguments = [
        trap_frame.rdi,
        trap_frame.rsi,
        trap_frame.rdx,
        trap_frame.r10,
        trap_frame.r8,
        trap_frame.r9,
    ];
    let result = if syscall_number == SYS_EXECVE {
        sys_execve(arguments[0], arguments[1], arguments[2], Some(trap_frame))
    } else {
        dispatch_syscall(syscall_number, arguments)
    };
    record_syscall_trace(task_id, syscall_number, arguments, result);
    if result == USER_EXECVE_SENTINEL {
        return USER_EXECVE_SENTINEL;
    }
    if result == USER_BLOCK_SENTINEL {
        trap_frame.rax = 0;
        crate::kernel::task::record_current_user_trap_frame(
            *trap_frame,
            trap_frame_storage_address,
            UserTrapFrameSource::Syscall,
        );
        let task_id = crate::kernel::task::block_current_user_after_syscall()
            .expect("prepared blocking syscall must block after saving the syscall frame");
        crate::kernel::task::close_user_return_preemption_window(task_id);
        return USER_BLOCK_SENTINEL;
    }

    trap_frame.rax = result;
    if result != USER_EXIT_SENTINEL {
        crate::kernel::task::record_current_user_trap_frame(
            *trap_frame,
            trap_frame_storage_address,
            UserTrapFrameSource::Syscall,
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

    match with_current_file_descriptor_table(|file_descriptors| {
        file_descriptors.write(file_descriptor, buffer)
    }) {
        Ok(bytes_written) => u64::try_from(bytes_written).unwrap_or(u64::MAX),
        Err(error) => error,
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

    let close_on_exec = flags & contract::OPEN_CLOSE_ON_EXEC != 0;
    let Some(path) = resolve_current_process_path(&path) else {
        return ERROR_NOT_IMPLEMENTED;
    };

    match with_current_file_descriptor_table(|file_descriptors| {
        crate::kernel::filesystem::open_with_close_on_exec_in(
            file_descriptors,
            &path,
            close_on_exec,
        )
    }) {
        Ok(file_descriptor) => u64::try_from(file_descriptor).unwrap_or(u64::MAX),
        Err(error) => error,
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

fn with_current_file_descriptor_table<R>(
    operation: impl FnOnce(
        &mut crate::kernel::filesystem::FileDescriptorTable,
    ) -> crate::kernel::filesystem::FileSystemResult<R>,
) -> Result<R, u64> {
    crate::kernel::task::with_current_file_descriptor_table(operation)
        .ok_or(ERROR_NOT_IMPLEMENTED)?
        .map_err(filesystem_error_to_linux)
}

fn sys_chdir(user_path_pointer: u64) -> u64 {
    let Some(path) = copy_path_argument(user_path_pointer) else {
        return ERROR_BAD_ADDRESS;
    };
    let Some(path) = resolve_current_process_path(&path) else {
        return ERROR_NOT_IMPLEMENTED;
    };

    match crate::kernel::filesystem::metadata(&path) {
        Ok(metadata) if metadata.file_type == crate::kernel::filesystem::FileType::Directory => {
            if crate::kernel::task::set_current_working_directory(path.clone()).is_none() {
                return ERROR_NOT_IMPLEMENTED;
            }
            crate::log_info!("syscall", "chdir -> path={}", path);
            0
        }
        Ok(_) => ERROR_NOT_DIRECTORY,
        Err(error) => filesystem_error_to_linux(error),
    }
}

fn sys_spawn(
    user_path_pointer: u64,
    user_argument_values_pointer: u64,
    user_environment_values_pointer: u64,
) -> u64 {
    let staging = match copy_user_program_entry_staging(
        user_path_pointer,
        user_argument_values_pointer,
        user_environment_values_pointer,
    ) {
        Ok(staging) => staging,
        Err(error) => return error,
    };

    let copied_argument_values = staging
        .argument_values
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    let default_argument_values = [staging.path.as_bytes()];
    let argument_values = if copied_argument_values.is_empty() {
        &default_argument_values[..]
    } else {
        copied_argument_values.as_slice()
    };
    let environment_values = staging
        .environment_values
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    let entry_vectors =
        crate::kernel::process::UserProgramEntryVectors::new(argument_values, &environment_values);
    let Some(descriptor_inheritance) =
        crate::kernel::task::get_current_spawn_descriptor_inheritance_snapshot()
    else {
        return ERROR_NOT_IMPLEMENTED;
    };
    let request = crate::kernel::process::UserProgramSpawnRequest::new(
        &staging.path,
        entry_vectors,
        SPAWN_USER_STACK_PAGES,
    );

    let Some(result) = crate::kernel::memory::runtime_allocator::with_user_runtime_frame_allocator(
        |frame_allocator| crate::kernel::process::spawn_user_program(frame_allocator, request),
    ) else {
        return ERROR_NOT_IMPLEMENTED;
    };
    let child_task_id = match result {
        Ok(child_task_id) => child_task_id,
        Err(error) => return spawn_error_to_linux(error),
    };
    if !crate::kernel::task::activate_user_task(child_task_id) {
        return ERROR_NOT_IMPLEMENTED;
    }

    crate::log_info!(
        "syscall",
        "spawn -> child={} path={} argc={} envc={} stack_pages={}",
        child_task_id,
        staging.path,
        entry_vectors.argument_count(),
        entry_vectors.environment_count(),
        SPAWN_USER_STACK_PAGES.as_u64()
    );
    crate::log_info!(
        "syscall",
        "spawn descriptor inheritance selected -> child={} inherited={} standard={} close_on_exec={} process_table=true",
        child_task_id,
        descriptor_inheritance.inherited_descriptors(),
        descriptor_inheritance.standard_descriptors(),
        descriptor_inheritance.close_on_exec_descriptors()
    );
    child_task_id
}

fn spawn_error_to_linux(error: crate::kernel::process::UserProgramSpawnError) -> u64 {
    match error {
        crate::kernel::process::UserProgramSpawnError::NotFound => ERROR_NOT_FOUND,
        crate::kernel::process::UserProgramSpawnError::InvalidPath
        | crate::kernel::process::UserProgramSpawnError::ReadFailed
        | crate::kernel::process::UserProgramSpawnError::InvalidImage => ERROR_INVALID_ARGUMENT,
        crate::kernel::process::UserProgramSpawnError::OutOfMemory => ERROR_OUT_OF_MEMORY,
        crate::kernel::process::UserProgramSpawnError::DirectoryTarget => ERROR_IS_DIRECTORY,
        crate::kernel::process::UserProgramSpawnError::UnsupportedTarget => ERROR_NOT_SUPPORTED,
    }
}

fn sys_getcwd(user_buffer_pointer: u64, buffer_length: u64) -> u64 {
    let Some(current_working_directory) = crate::kernel::task::get_current_working_directory()
    else {
        return ERROR_NOT_IMPLEMENTED;
    };
    let Some(required_length) = current_working_directory.len().checked_add(1) else {
        return ERROR_RANGE;
    };
    let Ok(required_length_u64) = u64::try_from(required_length) else {
        return ERROR_RANGE;
    };
    if buffer_length < required_length_u64 {
        return ERROR_RANGE;
    }

    let Some(buffer) = copy_output_buffer(user_buffer_pointer, required_length_u64) else {
        return ERROR_BAD_ADDRESS;
    };
    let path_bytes = current_working_directory.as_bytes();
    buffer[..path_bytes.len()].copy_from_slice(path_bytes);
    buffer[path_bytes.len()] = 0;
    crate::log_info!(
        "syscall",
        "getcwd -> path={} bytes={}",
        current_working_directory,
        required_length_u64
    );
    required_length_u64
}

fn sys_close(file_descriptor: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };

    match with_current_file_descriptor_table(|file_descriptors| {
        file_descriptors.close(file_descriptor)
    }) {
        Ok(()) => 0,
        Err(error) => error,
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

    match with_current_file_descriptor_table(|file_descriptors| {
        file_descriptors.metadata(file_descriptor)
    }) {
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
        Err(error) => error,
    }
}

fn sys_read(file_descriptor: u64, user_pointer: u64, length: u64) -> u64 {
    let Ok(file_descriptor) = usize::try_from(file_descriptor) else {
        return ERROR_BAD_FILE_DESCRIPTOR;
    };

    let Some(buffer) = copy_output_buffer(user_pointer, length) else {
        return ERROR_BAD_ADDRESS;
    };

    match with_current_file_descriptor_table(|file_descriptors| {
        file_descriptors.read(file_descriptor, buffer)
    }) {
        Ok(bytes_read) => u64::try_from(bytes_read).unwrap_or(u64::MAX),
        Err(ERROR_TRY_AGAIN) => {
            if crate::kernel::task::prepare_current_user_read(
                crate::kernel::task::UserReadRequest::new(file_descriptor, user_pointer, length),
            )
            .is_none()
            {
                return ERROR_TRY_AGAIN;
            }
            USER_BLOCK_SENTINEL
        }
        Err(error) => error,
    }
}

/// Complete a pending user `read` request after switching to the task address space.
pub fn complete_pending_user_read(task_id: u64) -> Option<u64> {
    let request = crate::kernel::task::take_current_user_read_request(task_id)?;
    let result = complete_user_read_request(request);
    let _ = crate::kernel::task::complete_current_user_read(task_id, result);
    Some(result)
}

fn complete_user_read_request(request: crate::kernel::task::UserReadRequest) -> u64 {
    let Some(buffer) = copy_output_buffer(request.user_pointer(), request.byte_len()) else {
        return ERROR_BAD_ADDRESS;
    };

    match with_current_file_descriptor_table(|file_descriptors| {
        file_descriptors.read(request.file_descriptor(), buffer)
    }) {
        Ok(bytes_read) => u64::try_from(bytes_read).unwrap_or(u64::MAX),
        Err(error) => error,
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
        match with_current_file_descriptor_table(|file_descriptors| {
            file_descriptors.read_directory(file_descriptor)
        }) {
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
            Err(error) => return error,
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

    match with_current_file_descriptor_table(|file_descriptors| {
        file_descriptors.seek_from(file_descriptor, offset, whence)
    }) {
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
        Err(error) => error,
    }
}

fn sys_execve(
    user_path_pointer: u64,
    user_argument_values_pointer: u64,
    user_environment_values_pointer: u64,
    replacement_trap_frame: Option<&mut UserTrapFrame>,
) -> u64 {
    let staging = match copy_user_program_entry_staging(
        user_path_pointer,
        user_argument_values_pointer,
        user_environment_values_pointer,
    ) {
        Ok(staging) => staging,
        Err(error) => return error,
    };
    let executable_image = match read_execve_candidate_image(&staging.path) {
        Ok(image) => image,
        Err(error) => return error,
    };
    if !crate::kernel::elf::validate_user_program_image(&executable_image, &staging.path) {
        return ERROR_INVALID_ARGUMENT;
    }
    let Some(replacement_trap_frame) = replacement_trap_frame else {
        crate::log_debug!(
            "syscall",
            "execve(path={}, image_bytes={}, argc={}, envc={}, bytes={}) requires a trap frame",
            staging.path,
            executable_image.len(),
            staging.argument_values.len(),
            staging.environment_values.len(),
            staging.copied_string_bytes
        );
        return ERROR_NOT_IMPLEMENTED;
    };

    let Some(published) =
        crate::kernel::memory::runtime_allocator::with_user_runtime_frame_allocator(
            |frame_allocator| {
                build_and_publish_execve_candidate(frame_allocator, &staging, &executable_image)
            },
        )
        .flatten()
    else {
        return ERROR_NOT_IMPLEMENTED;
    };
    *replacement_trap_frame = published.trap_frame;

    crate::log_info!(
        "syscall",
        "execve image published -> task={} path={} entry={:#x} stack={:#x} heap_start={:#x} argc={} old_user_pages={} old_page_table_pages={}",
        published.task_id,
        staging.path,
        published.entry_point,
        published.stack_pointer,
        published.heap_start,
        published.argument_count,
        published.reclaimed_old_user_pages,
        published.reclaimed_old_page_table_pages
    );

    USER_EXECVE_SENTINEL
}

struct ExecveImageCandidate {
    address_space: crate::kernel::memory::address_space::UserAddressSpace,
    trap_frame: UserTrapFrame,
    heap_start: crate::kernel::memory::address::UserVirtualAddress,
    argument_count: u64,
}

struct ExecvePublishedImage {
    task_id: u64,
    entry_point: u64,
    stack_pointer: u64,
    heap_start: u64,
    argument_count: u64,
    trap_frame: UserTrapFrame,
    reclaimed_old_user_pages: u64,
    reclaimed_old_page_table_pages: u64,
}

fn build_and_publish_execve_candidate(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    staging: &UserProgramEntryStaging,
    executable_image: &[u8],
) -> Option<ExecvePublishedImage> {
    let current_address_space = crate::kernel::task::get_current_user_address_space();
    let candidate = build_execve_candidate(frame_allocator, staging, executable_image);
    let Some((task_id, old_address_space)) = crate::kernel::task::replace_current_user_image(
        candidate.address_space,
        candidate.trap_frame,
        candidate.heap_start,
        &staging.path,
    ) else {
        let reclaim = crate::kernel::memory::address_space::destroy_user_address_space(
            frame_allocator,
            candidate.address_space,
        );
        if let Some(current_address_space) = current_address_space {
            crate::kernel::memory::address_space::switch_to_user_address_space(
                current_address_space,
            );
        }
        crate::kernel::task::record_current_user_execve_candidate_drop();
        crate::log_warn!(
            "syscall",
            "execve candidate dropped -> path={} user_pages={} page_table_pages={}",
            staging.path,
            reclaim.user_pages(),
            reclaim.page_table_pages()
        );
        return None;
    };

    let reclaim = crate::kernel::memory::address_space::destroy_user_address_space(
        frame_allocator,
        old_address_space,
    );
    assert!(
        crate::kernel::task::record_current_user_execve_reclaim(task_id, reclaim),
        "published execve task must retain reclaim diagnostics"
    );
    let closed_descriptors = crate::kernel::task::close_current_file_descriptors_on_exec()
        .expect("published execve task must retain a file descriptor table");
    if closed_descriptors > 0 {
        crate::log_info!(
            "syscall",
            "close-on-exec descriptors closed: count={}",
            closed_descriptors
        );
    }
    crate::kernel::memory::address_space::switch_to_user_address_space(candidate.address_space);

    Some(ExecvePublishedImage {
        task_id,
        entry_point: candidate.trap_frame.instruction_pointer,
        stack_pointer: candidate.trap_frame.stack_pointer,
        heap_start: candidate.heap_start.as_u64(),
        argument_count: candidate.argument_count,
        trap_frame: candidate.trap_frame,
        reclaimed_old_user_pages: reclaim.user_pages(),
        reclaimed_old_page_table_pages: reclaim.page_table_pages(),
    })
}

fn build_execve_candidate(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    staging: &UserProgramEntryStaging,
    executable_image: &[u8],
) -> ExecveImageCandidate {
    let candidate_address_space =
        crate::kernel::memory::address_space::create_user_address_space(frame_allocator);
    let loaded = crate::kernel::elf::load_user_program(
        candidate_address_space,
        frame_allocator,
        executable_image,
        &staging.path,
    );
    let candidate_stack = crate::kernel::memory::user_stack::allocate_user_stack(
        candidate_address_space,
        frame_allocator,
        EXECVE_USER_STACK_PAGES,
    );
    let argument_values = staging
        .argument_values
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    let environment_values = staging
        .environment_values
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    let prepared_stack = crate::kernel::memory::user_stack::prepare_initial_stack_bytes(
        candidate_address_space,
        candidate_stack,
        &argument_values,
        &environment_values,
    );
    let entry_arguments = crate::kernel::task::UserEntryArguments::new(
        prepared_stack.argument_count(),
        prepared_stack.argument_values_pointer(),
        prepared_stack.environment_values_pointer(),
    );
    // SAFETY: The candidate ELF loader mapped the entry point in
    // `candidate_address_space`, and `prepare_initial_stack_bytes` returned a
    // stack pointer inside the freshly mapped writable user stack.
    let entry_context = unsafe {
        crate::kernel::task::context::UserTaskContext::new(
            loaded.entry_point(),
            prepared_stack.stack_pointer(),
            entry_arguments,
        )
    };
    let trap_frame = entry_context.to_trap_frame();

    ExecveImageCandidate {
        address_space: candidate_address_space,
        trap_frame,
        heap_start: loaded.heap_start(),
        argument_count: prepared_stack.argument_count(),
    }
}

struct UserProgramEntryStaging {
    path: String,
    argument_values: Vec<Vec<u8>>,
    environment_values: Vec<Vec<u8>>,
    copied_string_bytes: usize,
}

fn copy_user_program_entry_staging(
    user_path_pointer: u64,
    user_argument_values_pointer: u64,
    user_environment_values_pointer: u64,
) -> Result<UserProgramEntryStaging, u64> {
    let Some(path) = copy_path_argument(user_path_pointer) else {
        return Err(ERROR_BAD_ADDRESS);
    };
    let Some(path) = resolve_current_process_path(&path) else {
        return Err(ERROR_NOT_IMPLEMENTED);
    };

    let mut copied_string_bytes = 0;
    let argument_values = copy_user_entry_string_vector(
        user_argument_values_pointer,
        MAX_USER_ENTRY_ARGUMENT_COUNT,
        &mut copied_string_bytes,
    )?;
    let environment_values = copy_user_entry_string_vector(
        user_environment_values_pointer,
        MAX_USER_ENTRY_ENVIRONMENT_COUNT,
        &mut copied_string_bytes,
    )?;

    Ok(UserProgramEntryStaging {
        path,
        argument_values,
        environment_values,
        copied_string_bytes,
    })
}

fn read_execve_candidate_image(path: &str) -> Result<Vec<u8>, u64> {
    if !path.starts_with('/') {
        return Err(ERROR_INVALID_ARGUMENT);
    }

    let metadata = crate::kernel::filesystem::metadata(path).map_err(filesystem_error_to_linux)?;
    match metadata.file_type {
        crate::kernel::filesystem::FileType::Regular => {}
        crate::kernel::filesystem::FileType::Directory => return Err(ERROR_IS_DIRECTORY),
        crate::kernel::filesystem::FileType::Device => return Err(ERROR_NOT_SUPPORTED),
    }

    let descriptor = crate::kernel::filesystem::open(path).map_err(filesystem_error_to_linux)?;
    let result = read_execve_descriptor_image(descriptor, metadata.size);
    if let Err(error) = crate::kernel::filesystem::close(descriptor) {
        panic!("failed to close execve image descriptor for {path}: {error:?}");
    }

    result
}

fn resolve_current_process_path(path: &str) -> Option<String> {
    let current_working_directory = crate::kernel::task::get_current_working_directory()?;
    Some(crate::kernel::filesystem::resolve_path(
        &current_working_directory,
        path,
    ))
}

fn read_execve_descriptor_image(file_descriptor: usize, byte_len: usize) -> Result<Vec<u8>, u64> {
    let mut image = Vec::new();
    image
        .try_reserve_exact(byte_len)
        .map_err(|_| ERROR_OUT_OF_MEMORY)?;
    image.resize(byte_len, 0);

    let mut bytes_read = 0_usize;
    while bytes_read < byte_len {
        let read_now = crate::kernel::filesystem::read(file_descriptor, &mut image[bytes_read..])
            .map_err(filesystem_error_to_linux)?;
        if read_now == 0 {
            return Err(ERROR_INVALID_ARGUMENT);
        }
        bytes_read = bytes_read
            .checked_add(read_now)
            .ok_or(ERROR_INVALID_ARGUMENT)?;
    }

    Ok(image)
}

fn copy_user_entry_string_vector(
    user_pointer_array: u64,
    max_values: usize,
    copied_string_bytes: &mut usize,
) -> Result<Vec<Vec<u8>>, u64> {
    if user_pointer_array == 0 {
        return Ok(Vec::new());
    }

    let mut values = Vec::new();
    for index in 0..=max_values {
        let string_pointer = copy_user_entry_pointer_array_slot(user_pointer_array, index)?;
        if string_pointer == 0 {
            return Ok(values);
        }
        if index == max_values {
            return Err(ERROR_ARGUMENT_LIST_TOO_LONG);
        }

        values.push(copy_user_entry_string_value(
            string_pointer,
            copied_string_bytes,
        )?);
    }

    Ok(values)
}

fn copy_user_entry_pointer_array_slot(user_pointer_array: u64, index: usize) -> Result<u64, u64> {
    let offset = u64::try_from(index)
        .ok()
        .and_then(|index| index.checked_mul(USER_POINTER_BYTES_U64))
        .ok_or(ERROR_BAD_ADDRESS)?;
    let slot_pointer = user_pointer_array
        .checked_add(offset)
        .ok_or(ERROR_BAD_ADDRESS)?;
    let Some(buffer) = copy_input_buffer(slot_pointer, USER_POINTER_BYTES_U64) else {
        return Err(ERROR_BAD_ADDRESS);
    };

    Ok(read_user_u64(buffer, 0))
}

fn copy_user_entry_string_value(
    user_string_pointer: u64,
    copied_string_bytes: &mut usize,
) -> Result<Vec<u8>, u64> {
    let range = UserVirtualRange::from_syscall_arguments(
        user_string_pointer,
        u64::try_from(MAX_USER_ENTRY_COPIED_STRING_BYTES)
            .expect("max user entry string bytes must fit in u64"),
    )
    .ok_or(ERROR_BAD_ADDRESS)?;
    let Some(value) =
        user_pointer::copy_cstr_bytes_from_user(UserCString::new(UserReadableRange::new(range)))
    else {
        return Err(ERROR_BAD_ADDRESS);
    };

    let next_string_bytes = value
        .len()
        .checked_add(1)
        .and_then(|length| copied_string_bytes.checked_add(length))
        .ok_or(ERROR_ARGUMENT_LIST_TOO_LONG)?;
    if next_string_bytes > MAX_USER_ENTRY_COPIED_STRING_BYTES {
        return Err(ERROR_ARGUMENT_LIST_TOO_LONG);
    }

    *copied_string_bytes = next_string_bytes;
    Ok(value)
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

fn validate_output_buffer(user_pointer: u64, byte_len: u64) -> bool {
    copy_output_buffer(user_pointer, byte_len).is_some()
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

fn write_user_u32(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + core::mem::size_of::<u32>()].copy_from_slice(&value.to_ne_bytes());
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
        crate::kernel::filesystem::FileSystemError::WouldBlock => ERROR_TRY_AGAIN,
    }
}

const fn page_count(count: u64) -> PageCount {
    PageCount::new(count).expect("syscall user stack page count must be valid")
}
