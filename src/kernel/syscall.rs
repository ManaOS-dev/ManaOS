//! # `kernel::syscall`
//!
//! ## Owns
//! - Kernel syscall number definitions
//! - Syscall argument dispatch
//! - Mapping kernel filesystem errors to Linux-like syscall results
//!
//! ## Does NOT own
//! - Architecture-specific `SYSCALL`/`SYSRET` register entry
//! - User pointer validation (-> `kernel::memory::user_pointer`)
//! - Per-process file descriptor tables
//!
//! ## Public API
//! - [`syscall_dispatch`] - Dispatch one syscall from architecture entry code
//! - [`syscall_dispatch_from_trap_frame`] - Dispatch one syscall from a saved user frame
//! - [`SYS_READ`] - Linux-compatible read syscall number
//! - [`SYS_WRITE`] - Linux-compatible write syscall number
//! - [`SYS_OPEN`] - Linux-compatible open syscall number
//! - [`SYS_CLOSE`] - Linux-compatible close syscall number
//! - [`SYS_FSTAT`] - Linux-compatible file status syscall number
//! - [`SYS_LSEEK`] - Linux-compatible seek syscall number
//! - [`SYS_MMAP`] - Linux-compatible anonymous memory-map syscall number
//! - [`SYS_MUNMAP`] - Linux-compatible memory-unmap syscall number
//! - [`SYS_BRK`] - Linux-compatible heap break syscall number
//! - [`SYS_EXIT`] - Linux-compatible exit syscall number
//! - [`SYS_GETDENTS64`] - Linux-compatible get-directory-entries syscall number
//! - [`SYS_EXIT_GROUP`] - Linux-compatible process exit syscall number
//! - [`SYS_GETPID`] - Linux-compatible get-process-identifier syscall number
//! - [`SYS_OPENAT`] - Linux-compatible open-at syscall number

use crate::kernel::memory::{
    address::{UserCString, UserReadableRange, UserVirtualRange, UserWritableRange},
    user_mapping::{UserMappingError, UserMappingPlacement},
    user_pointer,
};
use crate::kernel::task::context::UserTrapFrame;

#[allow(dead_code)]
#[path = "../shared/syscall_contract.rs"]
mod contract;

pub use contract::{
    SYS_BRK, SYS_CLOSE, SYS_EXIT, SYS_EXIT_GROUP, SYS_FSTAT, SYS_GETDENTS64, SYS_GETPID, SYS_LSEEK,
    SYS_MMAP, SYS_MUNMAP, SYS_OPEN, SYS_OPENAT, SYS_READ, SYS_WRITE,
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
const MAX_USER_STRING_LENGTH: usize = 256;
const PAGE_SIZE: u64 = 4096;
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
        SYS_MMAP => sys_mmap(
            first_argument,
            second_argument,
            third_argument,
            fourth_argument,
        ),
        SYS_MUNMAP => sys_munmap(first_argument, second_argument),
        SYS_BRK => sys_brk(first_argument),
        SYS_READ => sys_read(first_argument, second_argument, third_argument),
        SYS_GETDENTS64 => sys_getdents64(first_argument, second_argument, third_argument),
        SYS_GETPID => sys_getpid(),
        _ => ERROR_NOT_IMPLEMENTED,
    }
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
    );
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

fn sys_brk(requested_break: u64) -> u64 {
    crate::kernel::memory::runtime_allocator::with_user_runtime_frame_allocator(|frame_allocator| {
        crate::kernel::task::process_current_user_break(frame_allocator, requested_break)
    })
    .flatten()
    .unwrap_or(ERROR_NOT_IMPLEMENTED)
}

fn sys_mmap(requested_address: u64, length: u64, protection: u64, flags: u64) -> u64 {
    let Some(placement) = anonymous_mapping_placement(requested_address, flags) else {
        return ERROR_INVALID_ARGUMENT;
    };
    if !is_supported_anonymous_mapping_request(length, protection, flags) {
        return ERROR_INVALID_ARGUMENT;
    }

    let writable = protection & contract::PROT_WRITE != 0;
    let Some(result) = crate::kernel::memory::runtime_allocator::with_user_runtime_frame_allocator(
        |frame_allocator| {
            crate::kernel::task::process_current_user_mapping(
                frame_allocator,
                crate::kernel::task::UserMappingRequest::new(
                    requested_address,
                    placement,
                    length,
                    writable,
                    protection,
                    flags,
                ),
            )
        },
    ) else {
        return ERROR_NOT_IMPLEMENTED;
    };

    result.map_or(ERROR_NOT_IMPLEMENTED, |result| {
        result.unwrap_or_else(user_mapping_error_to_linux)
    })
}

fn sys_munmap(start_address: u64, length: u64) -> u64 {
    if start_address == 0 || length == 0 || !start_address.is_multiple_of(PAGE_SIZE) {
        return ERROR_INVALID_ARGUMENT;
    }

    let Some(result) = crate::kernel::memory::runtime_allocator::with_user_runtime_frame_allocator(
        |frame_allocator| {
            crate::kernel::task::process_current_user_unmapping(
                frame_allocator,
                start_address,
                length,
            )
        },
    ) else {
        return ERROR_NOT_IMPLEMENTED;
    };

    if result.is_some() {
        0
    } else {
        ERROR_INVALID_ARGUMENT
    }
}

fn is_supported_anonymous_mapping_request(length: u64, protection: u64, flags: u64) -> bool {
    let supported_protection = contract::PROT_READ | contract::PROT_WRITE | contract::PROT_EXEC;
    let supported_flags =
        contract::MAP_PRIVATE | contract::MAP_ANONYMOUS | contract::MAP_FIXED_NOREPLACE;
    length != 0
        && (protection & !supported_protection) == 0
        && (protection & contract::PROT_EXEC) == 0
        && (protection & (contract::PROT_READ | contract::PROT_WRITE)) != 0
        && (flags & !supported_flags) == 0
        && (flags & (contract::MAP_PRIVATE | contract::MAP_ANONYMOUS))
            == (contract::MAP_PRIVATE | contract::MAP_ANONYMOUS)
}

fn anonymous_mapping_placement(requested_address: u64, flags: u64) -> Option<UserMappingPlacement> {
    let fixed_no_replace = flags & contract::MAP_FIXED_NOREPLACE != 0;
    if fixed_no_replace {
        if requested_address == 0 || !requested_address.is_multiple_of(PAGE_SIZE) {
            return None;
        }
        let address = crate::kernel::memory::address::UserVirtualAddress::new(requested_address)?;
        Some(UserMappingPlacement::FixedNoReplace(address))
    } else if requested_address == 0 {
        Some(UserMappingPlacement::Any)
    } else {
        None
    }
}

fn user_mapping_error_to_linux(error: UserMappingError) -> u64 {
    match error {
        UserMappingError::InvalidRequest => ERROR_INVALID_ARGUMENT,
        UserMappingError::AddressInUse => ERROR_FILE_EXISTS,
        UserMappingError::OutOfMemory => ERROR_OUT_OF_MEMORY,
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
        crate::kernel::task::close_user_exit_preemption_window(task_id);
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
