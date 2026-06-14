//! # `kernel::process`
//!
//! ## Owns
//! - Kernel-internal user program spawn orchestration
//! - Filesystem path to scheduler task construction flow
//!
//! ## Does NOT own
//! - Filesystem namespace or descriptor state (-> `filesystem`)
//! - ELF validation and segment mapping policy (-> `elf`)
//! - User address-space and stack primitives (-> `memory`)
//! - Scheduler task records and parent-child metadata (-> `task`)
//!
//! ## Public API
//! - [`spawn_user_program`] - Spawn a user program from a filesystem path
//! - [`UserProgramSpawnRequest`] - User program spawn parameters
//! - [`UserProgramEntryVectors`] - User program argument and environment vectors
//! - [`UserProgramSpawnError`] - User program spawn failure reason

use crate::kernel::{
    elf::{self, LoadedElf},
    filesystem::{self, FileDescriptor, FileSystemError, FileType},
    memory::{
        address::UserVirtualAddress,
        address_space::{self, UserAddressSpace},
        frame_allocator::PhysicalFrameAllocator,
        user_stack::{self, AllocatedUserStack, PreparedUserStack},
    },
    task::{self, UserEntryArguments},
};
use alloc::vec::Vec;

/// Linux-compatible not-found errno for executable targets.
const ERROR_NOT_FOUND: isize = -2;
/// Linux-compatible is-directory errno for executable targets.
const ERROR_IS_DIRECTORY: isize = -21;
/// Linux-compatible invalid-argument errno for executable targets.
const ERROR_INVALID_ARGUMENT: isize = -22;
/// Linux-compatible out-of-memory errno for executable targets.
const ERROR_OUT_OF_MEMORY: isize = -12;
/// Linux-compatible operation-not-supported errno for executable targets.
const ERROR_NOT_SUPPORTED: isize = -95;

/// Parameters for spawning a user program from a filesystem path.
#[derive(Clone, Copy)]
pub struct UserProgramSpawnRequest<'a> {
    path: &'a str,
    entry_vectors: UserProgramEntryVectors<'a>,
    user_stack_pages: u64,
    kernel_probe_address: Option<usize>,
}

impl<'a> UserProgramSpawnRequest<'a> {
    /// Create a spawn request for a user program.
    pub const fn new(
        path: &'a str,
        entry_vectors: UserProgramEntryVectors<'a>,
        user_stack_pages: u64,
    ) -> Self {
        Self {
            path,
            entry_vectors,
            user_stack_pages,
            kernel_probe_address: None,
        }
    }

    /// Add a kernel address used for address-space permission self-checks.
    pub const fn with_kernel_probe_address(mut self, kernel_probe_address: usize) -> Self {
        self.kernel_probe_address = Some(kernel_probe_address);
        self
    }
}

/// User program argument and environment vectors before stack construction.
#[derive(Clone, Copy)]
pub struct UserProgramEntryVectors<'a> {
    arguments: &'a [&'a str],
    environment: &'a [&'a str],
}

impl<'a> UserProgramEntryVectors<'a> {
    /// Create user entry vectors from borrowed argument and environment slices.
    pub const fn new(arguments: &'a [&'a str], environment: &'a [&'a str]) -> Self {
        Self {
            arguments,
            environment,
        }
    }

    /// Return the argument vector used to build the initial user stack.
    pub const fn arguments(self) -> &'a [&'a str] {
        self.arguments
    }

    /// Return the environment vector used to build the initial user stack.
    pub const fn environment(self) -> &'a [&'a str] {
        self.environment
    }

    /// Return the number of user entry arguments.
    pub const fn argument_count(self) -> usize {
        self.arguments.len()
    }

    /// Return the number of user entry environment entries.
    pub const fn environment_count(self) -> usize {
        self.environment.len()
    }
}

/// User program spawn failure reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserProgramSpawnError {
    /// The executable path does not exist.
    NotFound,
    /// The executable path is not valid for the spawn model.
    InvalidPath,
    /// The executable path resolved to a directory.
    DirectoryTarget,
    /// The executable path resolved to an unsupported non-regular target.
    UnsupportedTarget,
    /// The executable contents could not be read after path lookup.
    ReadFailed,
    /// The executable image buffer could not be allocated.
    OutOfMemory,
    /// The executable image is not a supported user ELF.
    InvalidImage,
}

impl UserProgramSpawnError {
    /// Return the negative syscall-style errno result for this spawn failure.
    pub const fn as_syscall_result(self) -> isize {
        match self {
            Self::NotFound => ERROR_NOT_FOUND,
            Self::InvalidPath | Self::ReadFailed | Self::InvalidImage => ERROR_INVALID_ARGUMENT,
            Self::OutOfMemory => ERROR_OUT_OF_MEMORY,
            Self::DirectoryTarget => ERROR_IS_DIRECTORY,
            Self::UnsupportedTarget => ERROR_NOT_SUPPORTED,
        }
    }
}

/// Spawn a user program from a filesystem path.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized, user address-space setup
/// fails, or task kernel stack allocation fails.
pub fn spawn_user_program(
    frame_allocator: &mut PhysicalFrameAllocator,
    request: UserProgramSpawnRequest<'_>,
) -> Result<u64, UserProgramSpawnError> {
    let user_elf_bytes = load_program_image(request.path)?;
    let user_address_space = prepare_user_address_space(frame_allocator);
    let user_elf = load_user_elf(
        frame_allocator,
        user_address_space,
        &user_elf_bytes,
        request.path,
    );
    let user_entry_point = user_elf.entry_point();
    let user_heap_start = user_elf.heap_start();
    let user_stack = allocate_and_verify_user_stack(
        frame_allocator,
        user_address_space,
        request.user_stack_pages,
    );
    verify_user_program_mappings(
        user_address_space,
        user_stack,
        user_entry_point,
        request.kernel_probe_address,
    );
    log_user_entry_vectors(request.entry_vectors);
    let prepared_user_stack =
        prepare_user_entry_stack(user_address_space, user_stack, request.entry_vectors);
    let user_task_id = spawn_prepared_user_task(
        frame_allocator,
        user_address_space,
        user_entry_point,
        user_heap_start,
        prepared_user_stack,
        request.path,
    );
    crate::log_info!(
        "task",
        "User program spawned from filesystem: task={} path={} argc={}",
        user_task_id,
        request.path,
        prepared_user_stack.argument_count()
    );
    Ok(user_task_id)
}

fn log_user_entry_vectors(entry_vectors: UserProgramEntryVectors<'_>) {
    crate::log_info!(
        "task",
        "User program entry vectors staged: argument_count={} environment_count={}",
        entry_vectors.argument_count(),
        entry_vectors.environment_count()
    );
}

fn load_program_image(path: &str) -> Result<Vec<u8>, UserProgramSpawnError> {
    let user_elf_bytes = read_program_image(path)?;
    crate::log_info!(
        "elf",
        "Loading user ELF from filesystem: path={} bytes={}",
        path,
        user_elf_bytes.len()
    );
    if !elf::validate_user_program_image(&user_elf_bytes, path) {
        return Err(UserProgramSpawnError::InvalidImage);
    }
    Ok(user_elf_bytes)
}

fn prepare_user_address_space(frame_allocator: &mut PhysicalFrameAllocator) -> UserAddressSpace {
    let user_address_space = address_space::create_user_address_space(frame_allocator);
    crate::log_info!(
        "memory",
        "User address space prepared: pml4={:#x}",
        user_address_space.level_4_frame().as_u64()
    );
    user_address_space
}

fn load_user_elf(
    frame_allocator: &mut PhysicalFrameAllocator,
    user_address_space: UserAddressSpace,
    user_elf_bytes: &[u8],
    path: &str,
) -> LoadedElf {
    elf::load_user_program(user_address_space, frame_allocator, user_elf_bytes, path)
}

fn allocate_and_verify_user_stack(
    frame_allocator: &mut PhysicalFrameAllocator,
    user_address_space: UserAddressSpace,
    user_stack_pages: u64,
) -> AllocatedUserStack {
    let user_stack =
        user_stack::allocate_user_stack(user_address_space, frame_allocator, user_stack_pages);
    assert!(
        user_stack::verify_user_stack_mapping(user_address_space, user_stack),
        "user stack mapping and guard page smoke must pass"
    );
    crate::log_info!(
        "memory",
        "User stack mapping verified: pages={} base={:#x} top={:#x} guard_unmapped=true",
        user_stack.page_count(),
        user_stack.base().as_u64(),
        user_stack.top().as_u64()
    );
    user_stack
}

fn verify_user_program_mappings(
    user_address_space: UserAddressSpace,
    user_stack: AllocatedUserStack,
    user_entry_point: UserVirtualAddress,
    kernel_probe_address: Option<usize>,
) {
    let user_stack_probe = user_stack
        .top()
        .checked_sub(1)
        .expect("user stack top must be above the mapped stack");
    if let Some(kernel_probe_address) = kernel_probe_address {
        assert!(
            user_address_space.verify_kernel_user_mapping_permissions(
                kernel_probe_address,
                user_stack_probe.as_usize(),
                user_entry_point.as_usize(),
            ),
            "kernel and user mapping permission smoke must pass"
        );
        crate::log_info!(
            "memory",
            "Kernel/user mapping permission self-check passed: pml4={:#x}",
            user_address_space.level_4_frame().as_u64()
        );
    }
    assert!(
        user_address_space.verify_syscall_user_data_permissions(
            user_stack_probe.as_usize(),
            user_entry_point.as_usize(),
        ),
        "syscall user data permission smoke must pass"
    );
    crate::log_info!("memory", "Syscall user data permission self-check passed.");
}

fn prepare_user_entry_stack(
    user_address_space: UserAddressSpace,
    user_stack: AllocatedUserStack,
    entry_vectors: UserProgramEntryVectors<'_>,
) -> PreparedUserStack {
    let prepared_user_stack = user_stack::prepare_initial_stack(
        user_address_space,
        user_stack,
        entry_vectors.arguments(),
        entry_vectors.environment(),
    );
    crate::log_info!(
        "task",
        "User entry arguments prepared: argc={} argv={:#x} envp={:#x}",
        prepared_user_stack.argument_count(),
        prepared_user_stack.argument_values_pointer().as_u64(),
        prepared_user_stack.environment_values_pointer().as_u64()
    );
    prepared_user_stack
}

fn spawn_prepared_user_task(
    frame_allocator: &mut PhysicalFrameAllocator,
    user_address_space: UserAddressSpace,
    user_entry_point: UserVirtualAddress,
    user_heap_start: UserVirtualAddress,
    prepared_user_stack: PreparedUserStack,
    spawn_origin_path: &str,
) -> u64 {
    let user_task_id = task::spawn_user_task(
        frame_allocator,
        user_address_space,
        user_entry_point,
        prepared_user_stack.stack_pointer(),
        user_heap_start,
        UserEntryArguments::new(
            prepared_user_stack.argument_count(),
            prepared_user_stack.argument_values_pointer(),
            prepared_user_stack.environment_values_pointer(),
        ),
        spawn_origin_path,
    );
    crate::log_info!(
        "task",
        "User task spawned. task_id={} address_space={:#x}",
        user_task_id,
        user_address_space.level_4_frame().as_u64()
    );
    user_task_id
}

fn read_program_image(path: &str) -> Result<Vec<u8>, UserProgramSpawnError> {
    if !path.starts_with('/') {
        return Err(UserProgramSpawnError::InvalidPath);
    }

    let metadata = filesystem::metadata(path).map_err(map_filesystem_spawn_error)?;
    match metadata.file_type {
        FileType::Regular => {}
        FileType::Directory => return Err(UserProgramSpawnError::DirectoryTarget),
        FileType::Device => return Err(UserProgramSpawnError::UnsupportedTarget),
    }

    let descriptor = filesystem::open(path).map_err(map_filesystem_spawn_error)?;
    let mut contents = Vec::new();
    contents
        .try_reserve_exact(metadata.size)
        .map_err(|_| UserProgramSpawnError::OutOfMemory)?;
    contents.resize(metadata.size, 0);

    let read_result = read_exact_program_image(descriptor, &mut contents);
    let close_result = filesystem::close(descriptor);
    read_result?;
    close_result.map_err(|_| UserProgramSpawnError::ReadFailed)?;
    Ok(contents)
}

fn map_filesystem_spawn_error(error: FileSystemError) -> UserProgramSpawnError {
    match error {
        FileSystemError::NotFound => UserProgramSpawnError::NotFound,
        FileSystemError::InvalidPath | FileSystemError::InvalidArgument => {
            UserProgramSpawnError::InvalidPath
        }
        FileSystemError::IsDirectory => UserProgramSpawnError::DirectoryTarget,
        FileSystemError::UnsupportedOperation => UserProgramSpawnError::UnsupportedTarget,
        FileSystemError::InvalidFileDescriptor
        | FileSystemError::TooManyOpenFiles
        | FileSystemError::AlreadyInitialized
        | FileSystemError::NotDirectory => UserProgramSpawnError::ReadFailed,
    }
}

fn read_exact_program_image(
    descriptor: FileDescriptor,
    contents: &mut [u8],
) -> Result<(), UserProgramSpawnError> {
    let mut bytes_read = 0_usize;
    while bytes_read < contents.len() {
        let read_now = filesystem::read(descriptor, &mut contents[bytes_read..])
            .map_err(|_| UserProgramSpawnError::ReadFailed)?;
        if read_now == 0 {
            return Err(UserProgramSpawnError::ReadFailed);
        }
        bytes_read = bytes_read
            .checked_add(read_now)
            .expect("user program image read byte count overflowed");
    }
    Ok(())
}
