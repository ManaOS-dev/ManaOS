//! Memory-related syscall handlers.

use super::{
    contract, copy_input_buffer, copy_output_buffer, filesystem_error_to_linux, read_user_u64,
    ERROR_BAD_ADDRESS, ERROR_BAD_FILE_DESCRIPTOR, ERROR_FILE_EXISTS, ERROR_INVALID_ARGUMENT,
    ERROR_IS_DIRECTORY, ERROR_NOT_IMPLEMENTED, ERROR_NOT_SUPPORTED, ERROR_OUT_OF_MEMORY,
    NANOSECONDS_PER_SECOND, NANOSECONDS_PER_TIMER_TICK, PAGE_SIZE, USER_BLOCK_SENTINEL,
    USER_TIMESPEC_BYTES,
};
use crate::kernel::memory::{
    address::{UserVirtualAddress, VirtAddr},
    user_mapping::{UserMappingError, UserMappingPlacement, UserMappingSource},
};
/// Handle a user heap break syscall.
pub(super) fn sys_brk(requested_break: u64) -> u64 {
    crate::kernel::memory::runtime_allocator::with_user_runtime_frame_allocator(|frame_allocator| {
        crate::kernel::task::process_current_user_break(frame_allocator, requested_break)
    })
    .flatten()
    .unwrap_or(ERROR_NOT_IMPLEMENTED)
}

/// Handle a nanosleep syscall by preparing scheduler wake state.
pub(super) fn sys_nanosleep(request_pointer: u64, remaining_pointer: u64) -> u64 {
    let Some(request_buffer) = copy_input_buffer(
        request_pointer,
        u64::try_from(USER_TIMESPEC_BYTES).expect("user timespec size must fit in u64"),
    ) else {
        return ERROR_BAD_ADDRESS;
    };
    let request = read_user_timespec(request_buffer);
    let remaining_buffer = if remaining_pointer == 0 {
        None
    } else {
        let Some(buffer) = copy_output_buffer(
            remaining_pointer,
            u64::try_from(USER_TIMESPEC_BYTES).expect("user timespec size must fit in u64"),
        ) else {
            return ERROR_BAD_ADDRESS;
        };
        Some(buffer)
    };
    let Some(duration_ticks) = nanosleep_duration_ticks(request) else {
        return ERROR_INVALID_ARGUMENT;
    };
    if let Some(buffer) = remaining_buffer {
        buffer.fill(0);
    }
    if duration_ticks == 0 {
        return 0;
    }

    let Some(wake_tick) = crate::kernel::time::get_timer_ticks().checked_add(duration_ticks) else {
        return ERROR_INVALID_ARGUMENT;
    };
    if crate::kernel::task::prepare_current_user_sleep(wake_tick).is_none() {
        return ERROR_NOT_IMPLEMENTED;
    }

    USER_BLOCK_SENTINEL
}

fn read_user_timespec(buffer: &[u8]) -> contract::UserTimespec {
    contract::UserTimespec {
        seconds: read_user_u64(buffer, 0),
        nanoseconds: read_user_u64(buffer, 8),
    }
}

fn nanosleep_duration_ticks(request: contract::UserTimespec) -> Option<u64> {
    if request.nanoseconds >= NANOSECONDS_PER_SECOND {
        return None;
    }

    let second_ticks = request
        .seconds
        .checked_mul(crate::shared::TIMER_TICKS_PER_SECOND)?;
    let nanosecond_ticks = request.nanoseconds.div_ceil(NANOSECONDS_PER_TIMER_TICK);
    second_ticks.checked_add(nanosecond_ticks)
}

enum SyscallMappingSource {
    Anonymous,
    FilePrivate {
        file_descriptors: crate::kernel::filesystem::FileDescriptorTable,
        file_descriptor: usize,
        offset: usize,
    },
}

impl SyscallMappingSource {
    const fn user_mapping_source(&self) -> UserMappingSource {
        match self {
            Self::Anonymous => UserMappingSource::Anonymous,
            Self::FilePrivate { .. } => UserMappingSource::FilePrivate,
        }
    }
}

/// Handle a private memory mapping syscall.
pub(super) fn sys_mmap(
    requested_address: u64,
    length: u64,
    protection: u64,
    flags: u64,
    file_descriptor: u64,
    offset: u64,
) -> u64 {
    let Some(placement) = mapping_placement(requested_address, flags) else {
        return ERROR_INVALID_ARGUMENT;
    };
    if !is_supported_mapping_request(length, protection, flags) {
        return ERROR_INVALID_ARGUMENT;
    }
    let mapping_source = match mapping_source_from_arguments(file_descriptor, offset, flags) {
        Ok(mapping_source) => mapping_source,
        Err(error) => return error,
    };

    let writable = protection & contract::PROT_WRITE != 0;
    let request = crate::kernel::task::UserMappingRequest::new(
        requested_address,
        placement,
        mapping_source.user_mapping_source(),
        length,
        writable,
        protection,
        flags,
    );
    let mut file_read_error = None;
    let mut file_bytes_read = 0_usize;
    let Some(result) = crate::kernel::memory::runtime_allocator::with_user_runtime_frame_allocator(
        |frame_allocator| {
            crate::kernel::task::process_current_user_mapping(
                frame_allocator,
                request,
                |page_index, page_buffer| {
                    initialize_mapping_page(
                        &mapping_source,
                        page_index,
                        page_buffer,
                        &mut file_read_error,
                        &mut file_bytes_read,
                    )
                },
            )
        },
    ) else {
        return ERROR_NOT_IMPLEMENTED;
    };

    result.map_or(ERROR_NOT_IMPLEMENTED, |result| match result {
        Ok(start_address) => {
            if let SyscallMappingSource::FilePrivate {
                file_descriptor,
                offset,
                ..
            } = &mapping_source
            {
                crate::log_info!(
                    "syscall",
                    "mmap file preload -> fd={} offset={} start={:#x} length={} bytes={}",
                    file_descriptor,
                    offset,
                    start_address,
                    length,
                    file_bytes_read
                );
            }
            start_address
        }
        Err(UserMappingError::InitializationFailed) => {
            file_read_error.map_or(ERROR_NOT_SUPPORTED, filesystem_error_to_linux)
        }
        Err(error) => user_mapping_error_to_linux(error),
    })
}

/// Handle a private memory unmapping syscall.
pub(super) fn sys_munmap(start_address: u64, length: u64) -> u64 {
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

fn is_supported_mapping_request(length: u64, protection: u64, flags: u64) -> bool {
    let supported_protection = contract::PROT_READ | contract::PROT_WRITE | contract::PROT_EXEC;
    let supported_flags = contract::MAP_PRIVATE
        | contract::MAP_FIXED
        | contract::MAP_ANONYMOUS
        | contract::MAP_FIXED_NOREPLACE;
    length != 0
        && (protection & !supported_protection) == 0
        && (protection & contract::PROT_EXEC) == 0
        && (protection & (contract::PROT_READ | contract::PROT_WRITE)) != 0
        && (flags & !supported_flags) == 0
        && (flags & contract::MAP_PRIVATE) == contract::MAP_PRIVATE
}

fn mapping_placement(requested_address: u64, flags: u64) -> Option<UserMappingPlacement> {
    let fixed_no_replace = flags & contract::MAP_FIXED_NOREPLACE != 0;
    let fixed_replace = flags & contract::MAP_FIXED != 0;
    if fixed_no_replace {
        if requested_address == 0 || !requested_address.is_multiple_of(PAGE_SIZE) {
            return None;
        }
        let address = UserVirtualAddress::new(VirtAddr::new(requested_address))?;
        Some(UserMappingPlacement::FixedNoReplace(address))
    } else if fixed_replace {
        if requested_address == 0 || !requested_address.is_multiple_of(PAGE_SIZE) {
            return None;
        }
        let address = UserVirtualAddress::new(VirtAddr::new(requested_address))?;
        Some(UserMappingPlacement::FixedReplace(address))
    } else if requested_address == 0 {
        Some(UserMappingPlacement::Any)
    } else {
        None
    }
}

fn mapping_source_from_arguments(
    file_descriptor: u64,
    offset: u64,
    flags: u64,
) -> Result<SyscallMappingSource, u64> {
    if flags & contract::MAP_ANONYMOUS != 0 {
        return Ok(SyscallMappingSource::Anonymous);
    }

    if !offset.is_multiple_of(PAGE_SIZE) {
        return Err(ERROR_INVALID_ARGUMENT);
    }
    let file_descriptor =
        usize::try_from(file_descriptor).map_err(|_| ERROR_BAD_FILE_DESCRIPTOR)?;
    let offset = usize::try_from(offset).map_err(|_| ERROR_INVALID_ARGUMENT)?;
    let file_descriptors =
        crate::kernel::task::clone_current_file_descriptor_table().ok_or(ERROR_NOT_IMPLEMENTED)?;
    let metadata = file_descriptors
        .metadata(file_descriptor)
        .map_err(filesystem_error_to_linux)?;
    match metadata.file_type {
        crate::kernel::filesystem::FileType::Regular => Ok(SyscallMappingSource::FilePrivate {
            file_descriptors,
            file_descriptor,
            offset,
        }),
        crate::kernel::filesystem::FileType::Directory => Err(ERROR_IS_DIRECTORY),
        crate::kernel::filesystem::FileType::Device => Err(ERROR_NOT_SUPPORTED),
    }
}

fn initialize_mapping_page(
    mapping_source: &SyscallMappingSource,
    page_index: u64,
    page_buffer: &mut [u8],
    file_read_error: &mut Option<crate::kernel::filesystem::FileSystemError>,
    file_bytes_read: &mut usize,
) -> Result<(), UserMappingError> {
    let SyscallMappingSource::FilePrivate {
        file_descriptors,
        file_descriptor,
        offset,
    } = mapping_source
    else {
        return Ok(());
    };

    let page_offset = page_index
        .checked_mul(PAGE_SIZE)
        .and_then(|offset| usize::try_from(offset).ok())
        .ok_or(UserMappingError::InvalidRequest)?;
    let read_offset = (*offset)
        .checked_add(page_offset)
        .ok_or(UserMappingError::InvalidRequest)?;
    match file_descriptors.read_at(*file_descriptor, read_offset, page_buffer) {
        Ok(bytes_read) => {
            *file_bytes_read = file_bytes_read.saturating_add(bytes_read);
            Ok(())
        }
        Err(error) => {
            *file_read_error = Some(error);
            Err(UserMappingError::InitializationFailed)
        }
    }
}

fn user_mapping_error_to_linux(error: UserMappingError) -> u64 {
    match error {
        UserMappingError::InvalidRequest => ERROR_INVALID_ARGUMENT,
        UserMappingError::AddressInUse => ERROR_FILE_EXISTS,
        UserMappingError::OutOfMemory => ERROR_OUT_OF_MEMORY,
        UserMappingError::InitializationFailed => ERROR_NOT_SUPPORTED,
    }
}
