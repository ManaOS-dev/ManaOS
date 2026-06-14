//! Boot-time kernel smoke diagnostics.

use alloc::vec::Vec;

mod scheduler_diagnostics;

pub use scheduler_diagnostics::{
    record_memory_diagnostics_snapshot, verify_scheduler_task_diagnostics,
    verify_scheduler_task_snapshots,
};

fn read_kernel_file(path: &str) -> Option<Vec<u8>> {
    let metadata = crate::kernel::filesystem::metadata(path).ok()?;
    if metadata.file_type != crate::kernel::filesystem::FileType::Regular {
        return None;
    }

    let descriptor = crate::kernel::filesystem::open(path).ok()?;
    let mut contents = Vec::new();
    contents
        .try_reserve_exact(metadata.size)
        .expect("OOM: failed to reserve kernel file buffer");
    contents.resize(metadata.size, 0);

    let mut bytes_read = 0_usize;
    while bytes_read < metadata.size {
        let read_now =
            crate::kernel::filesystem::read(descriptor, &mut contents[bytes_read..]).ok()?;
        if read_now == 0 {
            break;
        }
        bytes_read = bytes_read
            .checked_add(read_now)
            .expect("kernel file read byte count overflowed");
    }
    crate::kernel::filesystem::close(descriptor).ok()?;
    contents.truncate(bytes_read);
    Some(contents)
}

fn spawn_user_smoke_task(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    user_elf_path: &str,
    user_elf_bytes: &[u8],
    user_stack_pages: u64,
) -> u64 {
    let user_address_space =
        crate::kernel::memory::address_space::create_user_address_space(frame_allocator);
    crate::log_info!(
        "memory",
        "User address space prepared: pml4={:#x}",
        user_address_space.level_4_frame().as_u64()
    );
    let user_elf: crate::kernel::elf::LoadedElf = crate::kernel::elf::load_user_program(
        user_address_space,
        frame_allocator,
        user_elf_bytes,
        user_elf_path,
    );
    let user_entry_point = user_elf.entry_point();
    let user_heap_start = user_elf.heap_start();
    let user_stack = crate::kernel::memory::user_stack::allocate_user_stack(
        user_address_space,
        frame_allocator,
        user_stack_pages,
    );
    assert!(
        crate::kernel::memory::user_stack::verify_user_stack_mapping(
            user_address_space,
            user_stack
        ),
        "user stack mapping and guard page smoke must pass"
    );
    crate::log_info!(
        "memory",
        "User stack mapping verified: pages={} base={:#x} top={:#x} guard_unmapped=true",
        user_stack.page_count(),
        user_stack.base().as_u64(),
        user_stack.top().as_u64()
    );

    let user_stack_probe = user_stack
        .top()
        .checked_sub(1)
        .expect("user stack top must be above the mapped stack");
    assert!(
        user_address_space.verify_kernel_user_mapping_permissions(
            run_user_smoke_demo as *const () as usize,
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
    assert!(
        user_address_space.verify_syscall_user_data_permissions(
            user_stack_probe.as_usize(),
            user_entry_point.as_usize(),
        ),
        "syscall user data permission smoke must pass"
    );
    crate::log_info!("memory", "Syscall user data permission self-check passed.");
    let user_entry_arguments = [user_elf_path, "--storage-smoke"];
    let user_entry_environment = ["MANAOS_BOOT=storage-smoke"];
    let prepared_user_stack = crate::kernel::memory::user_stack::prepare_initial_stack(
        user_address_space,
        user_stack,
        &user_entry_arguments,
        &user_entry_environment,
    );
    crate::log_info!(
        "task",
        "User entry arguments prepared: argc={} argv={:#x} envp={:#x}",
        prepared_user_stack.argument_count(),
        prepared_user_stack.argument_values_pointer().as_u64(),
        prepared_user_stack.environment_values_pointer().as_u64()
    );

    let user_task_id = crate::kernel::task::spawn_user_task(
        frame_allocator,
        user_address_space,
        user_entry_point,
        prepared_user_stack.stack_pointer(),
        user_heap_start,
        crate::kernel::task::UserEntryArguments::new(
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

/// Run the boot-time userland scheduler and syscall smoke demo.
pub fn run_user_smoke_demo(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    crate::kernel::task::set_preemption_enabled(false);

    let user_stack_pages = 4;
    let user_elf_path = "/disk/bin/smoke_demo";
    let user_elf_bytes =
        read_kernel_file(user_elf_path).expect("user smoke ELF must be readable from /disk/bin");
    crate::log_info!(
        "elf",
        "Loading user ELF from filesystem: path={} bytes={}",
        user_elf_path,
        user_elf_bytes.len()
    );
    let user_task_ids = [
        spawn_user_smoke_task(
            frame_allocator,
            user_elf_path,
            &user_elf_bytes,
            user_stack_pages,
        ),
        spawn_user_smoke_task(
            frame_allocator,
            user_elf_path,
            &user_elf_bytes,
            user_stack_pages,
        ),
    ];
    crate::log_info!(
        "task",
        "Multi-user smoke tasks spawned: first={} second={}",
        user_task_ids[0],
        user_task_ids[1]
    );
    for user_task_id in &user_task_ids {
        assert!(
            crate::kernel::task::activate_user_task(*user_task_id),
            "spawned user smoke task must be activatable"
        );
    }
    crate::log_info!(
        "task",
        "Multi-user active set prepared: tasks={}",
        user_task_ids.len()
    );

    let exits = crate::kernel::task::run_active_user_tasks_until_empty(frame_allocator);
    assert_eq!(
        exits.len(),
        user_task_ids.len(),
        "active user lifecycle drain must return every smoke task exit"
    );

    let mut finished = [false; 2];
    for exit in exits {
        crate::log_info!(
            "task",
            "UI resumed after user exit: task={} code={}",
            exit.task_id(),
            exit.exit_code()
        );
        let finished_index = user_task_ids
            .iter()
            .position(|task_id| *task_id == exit.task_id())
            .expect("exited task must belong to the multi-user smoke set");
        assert!(
            !finished[finished_index],
            "user smoke task must not exit twice"
        );
        finished[finished_index] = true;
    }

    assert!(
        finished.iter().all(|is_finished| *is_finished),
        "all user smoke tasks must exit"
    );
    verify_bootstrap_child_exit_collection(user_task_ids);
    crate::log_info!(
        "task",
        "Multi-user preemption smoke passed: tasks={}",
        user_task_ids.len()
    );
    crate::kernel::task::set_preemption_enabled(true);
}

fn verify_bootstrap_child_exit_collection(user_task_ids: [u64; 2]) {
    let parent_task_id = crate::kernel::task::TaskIdentifier::BOOTSTRAP.as_u64();
    let mut collected = [false; 2];
    for _ in 0..user_task_ids.len() {
        let exit = crate::kernel::task::collect_waitable_child_exit(parent_task_id)
            .expect("bootstrap parent must have a waitable user child exit");
        assert_eq!(
            exit.exit_code(),
            0,
            "user smoke child exit status must retain code zero"
        );
        let child_index = user_task_ids
            .iter()
            .position(|task_id| *task_id == exit.task_id())
            .expect("waited child must belong to the user smoke task set");
        assert!(
            !collected[child_index],
            "waited child exit status must be collected once"
        );
        collected[child_index] = true;
    }
    assert!(
        collected.iter().all(|is_collected| *is_collected),
        "bootstrap wait collection must cover every user smoke child"
    );
    assert!(
        crate::kernel::task::collect_waitable_child_exit(parent_task_id).is_none(),
        "bootstrap parent must not collect the same child exit twice"
    );
    crate::log_info!(
        "task",
        "Wait lifecycle smoke passed: parent={} retained_children={} collected_children={} double_reap_prevented=true",
        parent_task_id,
        user_task_ids.len(),
        collected.len()
    );
    crate::log_info!(
        "task",
        "Bootstrap child wait collection verified: parent={} children={}",
        parent_task_id,
        user_task_ids.len()
    );
}

/// Verify the scheduler diagnostics console command.
pub fn verify_scheduler_console_command() -> bool {
    match crate::kernel::console::verify_command_smoke_contains(
        "tasks",
        &[
            "reclaimed_user_address_spaces=",
            "process_lifecycle:",
            "collected_user_exit_statuses=",
            "zombie_user_tasks=",
            "reaped_user_tasks=",
            "one_shot_user_entries=",
            "timer_user_entries=",
            "user_vm_layout:",
            "task_image:",
            "path=/disk/bin/file_demo",
            "last_execve_old_user_pages=9",
            "task_vm:",
            "task_mmap_lifecycle:",
        ],
    ) {
        Some(output_lines) if output_lines >= 17 => {
            crate::log_info!(
                "console",
                "Tasks command smoke passed: command=\"tasks\" output_lines={}",
                output_lines
            );
            true
        }
        _ => {
            crate::log_warn!("console", "Tasks command smoke failed: command=\"tasks\"");
            false
        }
    }
}

/// Verify the memory diagnostics console command.
pub fn verify_memory_console_command() -> bool {
    match crate::kernel::console::verify_command_smoke("memory") {
        Some(output_lines) if output_lines >= 3 => {
            crate::log_info!(
                "console",
                "Memory command smoke passed: command=\"memory\" output_lines={}",
                output_lines
            );
            true
        }
        _ => {
            crate::log_warn!("console", "Memory command smoke failed: command=\"memory\"");
            false
        }
    }
}

/// Verify syscall trace console command controls.
pub fn verify_syscall_trace_console_command() -> bool {
    crate::kernel::syscall::set_trace_enabled(false);
    crate::kernel::syscall::reset_trace();
    let reset_ok = crate::kernel::console::verify_command_smoke_contains(
        "syscalls trace reset",
        &["trace: enabled=false", "records=0", "last_number=-"],
    )
    .is_some();
    let enabled_ok = crate::kernel::console::verify_command_smoke_contains(
        "syscalls trace on",
        &["trace: enabled=true", "records=0"],
    )
    .is_some();
    let _traced_result = crate::kernel::syscall::syscall_dispatch(
        crate::kernel::syscall::SYS_GETPID,
        0,
        0,
        0,
        0,
        0,
        0,
    );
    let disabled_ok = crate::kernel::console::verify_command_smoke_contains(
        "syscalls trace off",
        &[
            "trace: enabled=false",
            "records=1",
            "last_number=39",
            "last_result=0x",
        ],
    )
    .is_some();

    if reset_ok && enabled_ok && disabled_ok {
        crate::log_info!(
            "console",
            "Syscall trace controls smoke passed: command=\"syscalls trace\" records=1"
        );
        true
    } else {
        crate::log_warn!(
            "console",
            "Syscall trace controls smoke failed: command=\"syscalls trace\""
        );
        false
    }
}

/// Verify console status strip rendering diagnostics.
pub fn verify_console_status_strip() -> bool {
    if crate::kernel::console::verify_status_strip_smoke() {
        crate::log_info!("console", "Console status strip smoke passed.");
        true
    } else {
        crate::log_warn!("console", "Console status strip smoke failed.");
        false
    }
}
