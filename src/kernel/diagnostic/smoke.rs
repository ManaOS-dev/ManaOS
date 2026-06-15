//! Boot-time kernel smoke diagnostics.

mod scheduler_diagnostics;

pub use scheduler_diagnostics::{
    record_memory_diagnostics_snapshot, verify_scheduler_task_diagnostics,
    verify_scheduler_task_snapshots,
};

/// Number of active user processes spawned by the storage smoke lifecycle.
pub const USER_SMOKE_PARENT_TASK_COUNT: usize = 5;
/// Number of user-spawned child processes created by marked smoke parents.
pub const USER_SMOKE_CHILD_TASK_COUNT: usize = 2;
/// Exit code used by the blocking user-spawned child status smoke.
pub const USER_SMOKE_CHILD_EXIT_CODE: u64 = 7;
/// Exit code used by the child whose parent exits before it does.
pub const USER_SMOKE_ORPHAN_CHILD_EXIT_CODE: u64 = 43;
/// Number of user tasks expected after the full storage smoke lifecycle.
pub const USER_SMOKE_TASK_COUNT: usize = USER_SMOKE_PARENT_TASK_COUNT + USER_SMOKE_CHILD_TASK_COUNT;

const SPAWN_WAIT_PARENT_TASK_INDEX: usize = 3;
const ORPHAN_PARENT_TASK_INDEX: usize = 4;

fn spawn_user_smoke_task(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    user_elf_path: &str,
    user_stack_pages: u64,
) -> u64 {
    let user_entry_arguments: [&[u8]; 2] = [user_elf_path.as_bytes(), b"--storage-smoke"];
    let user_entry_environment: [&[u8]; 1] = [b"MANAOS_BOOT=storage-smoke"];
    let user_entry_vectors = crate::kernel::process::UserProgramEntryVectors::new(
        &user_entry_arguments,
        &user_entry_environment,
    );
    let request = crate::kernel::process::UserProgramSpawnRequest::new(
        user_elf_path,
        user_entry_vectors,
        user_stack_pages,
    )
    .with_kernel_probe_address(run_user_smoke_demo as *const () as usize);
    crate::kernel::process::spawn_user_program(frame_allocator, request)
        .expect("user smoke program must spawn from /disk/bin")
}

fn spawn_wait_user_smoke_task(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    user_stack_pages: u64,
) -> u64 {
    spawn_file_demo_marker_task(frame_allocator, user_stack_pages, b"--spawn-wait-smoke")
}

fn spawn_orphan_parent_user_smoke_task(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    user_stack_pages: u64,
) -> u64 {
    spawn_file_demo_marker_task(frame_allocator, user_stack_pages, b"--orphan-parent-smoke")
}

fn spawn_file_demo_marker_task(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    user_stack_pages: u64,
    marker_argument: &[u8],
) -> u64 {
    let user_elf_path = "/disk/bin/file_demo";
    let user_entry_arguments: [&[u8]; 2] = [user_elf_path.as_bytes(), marker_argument];
    let user_entry_environment: [&[u8]; 1] = [b"MANAOS_BOOT=storage-smoke"];
    let user_entry_vectors = crate::kernel::process::UserProgramEntryVectors::new(
        &user_entry_arguments,
        &user_entry_environment,
    );
    let request = crate::kernel::process::UserProgramSpawnRequest::new(
        user_elf_path,
        user_entry_vectors,
        user_stack_pages,
    );
    crate::kernel::process::spawn_user_program(frame_allocator, request)
        .expect("user file demo marker smoke program must spawn from /disk/bin")
}

/// Run the boot-time userland scheduler and syscall smoke demo.
pub fn run_user_smoke_demo(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    crate::kernel::task::set_preemption_enabled(false);

    let user_stack_pages = 4;
    let user_elf_path = "/disk/bin/smoke_demo";
    verify_spawn_path_errno_smoke(frame_allocator, user_stack_pages);
    let user_task_ids = [
        spawn_user_smoke_task(frame_allocator, user_elf_path, user_stack_pages),
        spawn_user_smoke_task(frame_allocator, user_elf_path, user_stack_pages),
        spawn_user_smoke_task(frame_allocator, user_elf_path, user_stack_pages),
        spawn_wait_user_smoke_task(frame_allocator, user_stack_pages),
        spawn_orphan_parent_user_smoke_task(frame_allocator, user_stack_pages),
    ];
    crate::log_info!(
        "task",
        "Multi-user smoke tasks spawned: first={} second={} third={} spawn_wait={} orphan_parent={}",
        user_task_ids[0],
        user_task_ids[1],
        user_task_ids[2],
        user_task_ids[SPAWN_WAIT_PARENT_TASK_INDEX],
        user_task_ids[ORPHAN_PARENT_TASK_INDEX]
    );
    assert_distinct_user_task_ids(user_task_ids);
    crate::log_info!(
        "task",
        "Concurrent user program spawn smoke passed: tasks={} first={} second={} third={} spawn_wait={} orphan_parent={}",
        user_task_ids.len(),
        user_task_ids[0],
        user_task_ids[1],
        user_task_ids[2],
        user_task_ids[SPAWN_WAIT_PARENT_TASK_INDEX],
        user_task_ids[ORPHAN_PARENT_TASK_INDEX]
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

    let exits = drain_user_smoke_tasks(frame_allocator, user_task_ids);
    assert_eq!(
        exits.len(),
        USER_SMOKE_TASK_COUNT,
        "active user lifecycle drain must return every smoke task exit"
    );

    verify_user_smoke_exits(user_task_ids, &exits);
    crate::log_info!(
        "task",
        "Multi-user preemption smoke passed: tasks={}",
        user_task_ids.len()
    );
    crate::kernel::task::set_preemption_enabled(true);
}

fn verify_user_smoke_exits(
    user_task_ids: [u64; USER_SMOKE_PARENT_TASK_COUNT],
    exits: &[crate::kernel::task::UserTaskExit],
) {
    let mut finished_parent_tasks = [false; USER_SMOKE_PARENT_TASK_COUNT];
    let mut finished_wait_child_tasks = 0_usize;
    let mut finished_orphan_child_tasks = 0_usize;
    for exit in exits {
        crate::log_info!(
            "task",
            "UI resumed after user exit: task={} code={}",
            exit.task_id(),
            exit.exit_code()
        );
        if let Some(finished_index) = user_task_ids
            .iter()
            .position(|task_id| *task_id == exit.task_id())
        {
            assert!(
                !finished_parent_tasks[finished_index],
                "user smoke parent task must not exit twice"
            );
            finished_parent_tasks[finished_index] = true;
        } else {
            match exit.exit_code() {
                USER_SMOKE_CHILD_EXIT_CODE => {
                    finished_wait_child_tasks = finished_wait_child_tasks.saturating_add(1);
                }
                USER_SMOKE_ORPHAN_CHILD_EXIT_CODE => {
                    finished_orphan_child_tasks = finished_orphan_child_tasks.saturating_add(1);
                }
                _ => panic!("user-spawned child task must retain a known nonzero smoke exit code"),
            }
        }
    }

    assert!(
        finished_parent_tasks.iter().all(|is_finished| *is_finished),
        "all user smoke parent tasks must exit"
    );
    assert_eq!(
        finished_wait_child_tasks, 1,
        "blocking wait smoke must finish one user-spawned child task"
    );
    assert_eq!(
        finished_orphan_child_tasks, 1,
        "parent-exit smoke must finish one user-spawned orphan child task"
    );
    assert_eq!(
        finished_wait_child_tasks + finished_orphan_child_tasks,
        USER_SMOKE_CHILD_TASK_COUNT,
        "all user-spawned child tasks must exit"
    );
    verify_parent_exit_child_live_smoke(user_task_ids, exits);
    verify_bootstrap_child_exit_collection(user_task_ids);
}

fn verify_parent_exit_child_live_smoke(
    user_task_ids: [u64; USER_SMOKE_PARENT_TASK_COUNT],
    exits: &[crate::kernel::task::UserTaskExit],
) {
    let orphan_parent_task_id = user_task_ids[ORPHAN_PARENT_TASK_INDEX];
    let parent_exit_index = exits
        .iter()
        .position(|exit| exit.task_id() == orphan_parent_task_id)
        .expect("orphan smoke parent must exit");
    let (child_exit_index, child_exit) = exits
        .iter()
        .enumerate()
        .find(|(_, exit)| exit.exit_code() == USER_SMOKE_ORPHAN_CHILD_EXIT_CODE)
        .expect("orphan smoke child must exit after parent");
    assert!(
        parent_exit_index < child_exit_index,
        "orphan smoke parent must exit before the child finishes"
    );
    assert!(
        crate::kernel::task::collect_waitable_child_exit(
            crate::kernel::task::TaskIdentifier::BOOTSTRAP.as_u64(),
            Some(child_exit.task_id()),
        )
        .is_none(),
        "orphan smoke child must not be reparented before the policy is implemented"
    );
    let retained_exit = crate::kernel::task::collect_waitable_child_exit(
        orphan_parent_task_id,
        Some(child_exit.task_id()),
    )
    .expect("orphan child exit must stay retained for the recorded parent");
    assert_eq!(
        retained_exit.exit_code(),
        USER_SMOKE_ORPHAN_CHILD_EXIT_CODE,
        "orphan child exit code must be retained under the original parent"
    );
    crate::log_info!(
        "task",
        "Parent-exit child-live smoke passed: parent={} child={} parent_exit_before_child=true retained_parent=true status={} reparented=false",
        orphan_parent_task_id,
        retained_exit.task_id(),
        retained_exit.wait_status()
    );
}

fn drain_user_smoke_tasks(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    user_task_ids: [u64; USER_SMOKE_PARENT_TASK_COUNT],
) -> alloc::vec::Vec<crate::kernel::task::UserTaskExit> {
    let mut exits = alloc::vec::Vec::new();
    while let Some(exit) = crate::kernel::task::run_next_user_task_once(frame_allocator) {
        if exits.is_empty() {
            verify_preempted_exit_continuation_smoke(exit, user_task_ids);
        }
        exits.push(exit);
    }
    crate::log_info!(
        "task",
        "Active user lifecycle drained: exits={}",
        exits.len()
    );
    exits
}

fn verify_preempted_exit_continuation_smoke(
    exit: crate::kernel::task::UserTaskExit,
    _user_task_ids: [u64; USER_SMOKE_PARENT_TASK_COUNT],
) {
    assert!(
        crate::kernel::task::has_active_user_tasks(),
        "at least one active user task must remain after the first user task exits"
    );
    let diagnostics = crate::kernel::task::get_scheduler_diagnostics()
        .expect("scheduler diagnostics must be available after the first user exit");
    assert!(
        diagnostics.timer_preemptions() > 0,
        "first user exit continuation smoke must include timer preemption"
    );
    let snapshots = crate::kernel::task::get_scheduler_task_snapshots()
        .expect("scheduler task snapshots must be available after the first user exit");
    let first_exit_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.task_id() == exit.task_id())
        .expect("first exited user task must have a retained scheduler snapshot");
    assert_ne!(
        first_exit_snapshot.last_resume_path(),
        crate::kernel::task::UserResumePathDiagnostics::None,
        "first exited user task must retain the last user resume path"
    );
    crate::log_info!(
        "task",
        "Preempted user exit continuation smoke passed: first_exit={} remaining_active=true timer_preemptions={} last_preemption_reason={} last_resume_path={}",
        exit.task_id(),
        diagnostics.timer_preemptions(),
        first_exit_snapshot.last_preemption_reason().as_str(),
        first_exit_snapshot.last_resume_path().as_str()
    );
}

fn assert_distinct_user_task_ids(user_task_ids: [u64; USER_SMOKE_PARENT_TASK_COUNT]) {
    for (current_index, current_task_id) in user_task_ids.iter().enumerate() {
        for next_task_id in user_task_ids.iter().skip(current_index + 1) {
            assert_ne!(
                *current_task_id, *next_task_id,
                "concurrent user spawn smoke tasks must be distinct"
            );
        }
    }
}

fn verify_spawn_path_errno_smoke(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    user_stack_pages: u64,
) {
    let missing_result = verify_spawn_error(
        frame_allocator,
        "/disk/bin/missing_spawn",
        user_stack_pages,
        crate::kernel::process::UserProgramSpawnError::NotFound,
    );
    let relative_result = verify_spawn_error(
        frame_allocator,
        "disk/bin/smoke_demo",
        user_stack_pages,
        crate::kernel::process::UserProgramSpawnError::InvalidPath,
    );
    let directory_result = verify_spawn_error(
        frame_allocator,
        "/disk",
        user_stack_pages,
        crate::kernel::process::UserProgramSpawnError::DirectoryTarget,
    );
    let device_result = verify_spawn_error(
        frame_allocator,
        "/dev/null",
        user_stack_pages,
        crate::kernel::process::UserProgramSpawnError::UnsupportedTarget,
    );
    let invalid_image_result = verify_spawn_error(
        frame_allocator,
        "/disk/hello.txt",
        user_stack_pages,
        crate::kernel::process::UserProgramSpawnError::InvalidImage,
    );
    let out_of_memory_result =
        crate::kernel::process::UserProgramSpawnError::OutOfMemory.as_syscall_result();
    crate::log_info!(
        "task",
        "User program spawn errno smoke passed: missing={} relative={} directory={} device={} invalid_image={} oom={}",
        missing_result,
        relative_result,
        directory_result,
        device_result,
        invalid_image_result,
        out_of_memory_result
    );
}

fn verify_spawn_error(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    path: &str,
    user_stack_pages: u64,
    expected_error: crate::kernel::process::UserProgramSpawnError,
) -> isize {
    let entry_arguments: [&[u8]; 0] = [];
    let entry_environment: [&[u8]; 0] = [];
    let entry_vectors =
        crate::kernel::process::UserProgramEntryVectors::new(&entry_arguments, &entry_environment);
    let request =
        crate::kernel::process::UserProgramSpawnRequest::new(path, entry_vectors, user_stack_pages);
    let error = crate::kernel::process::spawn_user_program(frame_allocator, request)
        .expect_err("invalid user spawn smoke path must fail before task creation");
    assert_eq!(
        error, expected_error,
        "spawn path failure must classify the expected error"
    );
    error.as_syscall_result()
}

fn verify_bootstrap_child_exit_collection(user_task_ids: [u64; USER_SMOKE_PARENT_TASK_COUNT]) {
    let parent_task_id = crate::kernel::task::TaskIdentifier::BOOTSTRAP.as_u64();
    let mut collected = [false; USER_SMOKE_PARENT_TASK_COUNT];
    let selected_exit =
        crate::kernel::task::collect_waitable_child_exit(parent_task_id, Some(user_task_ids[0]))
            .expect("bootstrap parent must collect the selected user child exit");
    assert_eq!(
        selected_exit.task_id(),
        user_task_ids[0],
        "selected child wait collection must return the requested child"
    );
    verify_user_child_exit(parent_task_id, &mut collected, user_task_ids, selected_exit);
    assert!(
        crate::kernel::task::collect_waitable_child_exit(parent_task_id, Some(user_task_ids[0]))
            .is_none(),
        "selected child exit status must not be collected twice"
    );
    crate::log_info!(
        "task",
        "Selected child wait collection verified: parent={} child={} status={}",
        parent_task_id,
        selected_exit.task_id(),
        selected_exit.wait_status()
    );

    collect_remaining_bootstrap_child_exits(parent_task_id, &mut collected, user_task_ids);
    assert!(
        collected.iter().all(|is_collected| *is_collected),
        "bootstrap wait collection must cover every user smoke child"
    );
    assert!(
        crate::kernel::task::collect_waitable_child_exit(parent_task_id, None).is_none(),
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

fn collect_remaining_bootstrap_child_exits(
    parent_task_id: u64,
    collected: &mut [bool; USER_SMOKE_PARENT_TASK_COUNT],
    user_task_ids: [u64; USER_SMOKE_PARENT_TASK_COUNT],
) {
    while !collected.iter().all(|is_collected| *is_collected) {
        let remaining_exit = crate::kernel::task::collect_waitable_child_exit(parent_task_id, None)
            .expect("bootstrap parent must have a remaining waitable user child exit");
        verify_user_child_exit(parent_task_id, collected, user_task_ids, remaining_exit);
    }
}

fn verify_user_child_exit(
    parent_task_id: u64,
    collected: &mut [bool; USER_SMOKE_PARENT_TASK_COUNT],
    user_task_ids: [u64; USER_SMOKE_PARENT_TASK_COUNT],
    exit: crate::kernel::task::UserTaskExit,
) {
    assert_eq!(
        exit.exit_code(),
        0,
        "user smoke child exit status must retain code zero"
    );
    assert_eq!(
        exit.wait_status(),
        0,
        "normal zero child exit must encode a zero wait status"
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
    crate::log_info!(
        "task",
        "Wait status verified: parent={} child={} code={} status={}",
        parent_task_id,
        exit.task_id(),
        exit.exit_code(),
        exit.wait_status()
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
            "lifecycle=reaped",
            "one_shot_user_entries=",
            "timer_user_entries=",
            "user_vm_layout:",
            "task_image:",
            "origin=/disk/bin/smoke_demo",
            "path=/disk/bin/file_demo",
            "last_execve_state=published",
            "last_execve_old_user_pages=9",
            "task_vm:",
            "task_mmap_lifecycle:",
            "last_preemption_reason=",
            "last_resume_path=",
        ],
    ) {
        Some(output_lines) if output_lines >= 21 => {
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
