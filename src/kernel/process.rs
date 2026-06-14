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
//! - [`UserProgramSpawnError`] - User program spawn failure reason

use crate::kernel::{
    elf::{self, LoadedElf},
    filesystem::{self, FileDescriptor, FileType},
    memory::{
        address::UserVirtualAddress,
        address_space::{self, UserAddressSpace},
        frame_allocator::PhysicalFrameAllocator,
        user_stack::{self, AllocatedUserStack, PreparedUserStack},
    },
    task::{self, UserEntryArguments},
};
use alloc::vec::Vec;

/// Parameters for spawning a user program from a filesystem path.
#[derive(Clone, Copy)]
pub struct UserProgramSpawnRequest<'a> {
    path: &'a str,
    arguments: &'a [&'a str],
    environment: &'a [&'a str],
    user_stack_pages: u64,
    kernel_probe_address: Option<usize>,
}

impl<'a> UserProgramSpawnRequest<'a> {
    /// Create a spawn request for a user program.
    pub const fn new(
        path: &'a str,
        arguments: &'a [&'a str],
        environment: &'a [&'a str],
        user_stack_pages: u64,
    ) -> Self {
        Self {
            path,
            arguments,
            environment,
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

/// User program spawn failure reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserProgramSpawnError {
    /// The executable path could not be read from the filesystem.
    ReadFailed,
    /// The executable path resolved to a non-regular file.
    NotRegularFile,
    /// The executable image is not a supported user ELF.
    InvalidImage,
}

/// Spawn a user program from a filesystem path.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized, user address-space setup
/// fails, task kernel stack allocation fails, or kernel heap allocation for the
/// executable image buffer fails.
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
    let prepared_user_stack = prepare_user_entry_stack(
        user_address_space,
        user_stack,
        request.arguments,
        request.environment,
    );
    let user_task_id = spawn_prepared_user_task(
        frame_allocator,
        user_address_space,
        user_entry_point,
        user_heap_start,
        prepared_user_stack,
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
    arguments: &[&str],
    environment: &[&str],
) -> PreparedUserStack {
    let prepared_user_stack =
        user_stack::prepare_initial_stack(user_address_space, user_stack, arguments, environment);
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
    let metadata = filesystem::metadata(path).map_err(|_| UserProgramSpawnError::ReadFailed)?;
    if metadata.file_type != FileType::Regular {
        return Err(UserProgramSpawnError::NotRegularFile);
    }

    let descriptor = filesystem::open(path).map_err(|_| UserProgramSpawnError::ReadFailed)?;
    let mut contents = Vec::new();
    contents
        .try_reserve_exact(metadata.size)
        .expect("OOM: failed to reserve user program image buffer");
    contents.resize(metadata.size, 0);

    let read_result = read_exact_program_image(descriptor, &mut contents);
    let close_result = filesystem::close(descriptor);
    read_result?;
    close_result.map_err(|_| UserProgramSpawnError::ReadFailed)?;
    Ok(contents)
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
