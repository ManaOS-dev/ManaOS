//! Boot-time kernel smoke diagnostics.

use alloc::vec::Vec;

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
        "Bootstrap child wait collection verified: parent={} children={}",
        parent_task_id,
        user_task_ids.len()
    );
}

/// Verify scheduler accounting after the userland smoke demo.
pub fn verify_scheduler_task_diagnostics(expected_user_tasks: u64) {
    let diagnostics = crate::kernel::task::get_scheduler_diagnostics()
        .expect("scheduler diagnostics must be available after user smoke tasks");
    let states = diagnostics.states();
    verify_scheduler_task_counts(&diagnostics, states, expected_user_tasks);
    verify_scheduler_reclaim_diagnostics(&diagnostics, expected_user_tasks);
    verify_scheduler_user_return_diagnostics(&diagnostics, expected_user_tasks);
    log_scheduler_task_diagnostics(&diagnostics, states);
}

fn verify_scheduler_task_counts(
    diagnostics: &crate::kernel::task::SchedulerDiagnostics,
    states: crate::kernel::task::TaskStateDiagnostics,
    expected_user_tasks: u64,
) {
    assert_eq!(
        diagnostics.user_tasks(),
        expected_user_tasks,
        "scheduler diagnostics must count spawned user tasks"
    );
    assert_eq!(
        diagnostics.active_user_address_spaces(),
        0,
        "finished user tasks must not retain address spaces"
    );
    assert_eq!(
        diagnostics.active_user_tasks(),
        0,
        "finished user tasks must not remain in the active scheduling set"
    );
    assert_eq!(
        diagnostics.retained_user_exit_statuses(),
        expected_user_tasks,
        "finished user tasks must retain waitable exit status records"
    );
    assert_eq!(
        diagnostics.waitable_user_exit_statuses(),
        0,
        "bootstrap parent must collect every waitable user child exit"
    );
    assert_eq!(
        diagnostics.collected_user_exit_statuses(),
        expected_user_tasks,
        "finished user task exit statuses must be marked collected"
    );
    assert_eq!(
        states.finished(),
        expected_user_tasks,
        "all user smoke tasks must be finished"
    );
}

fn verify_scheduler_reclaim_diagnostics(
    diagnostics: &crate::kernel::task::SchedulerDiagnostics,
    expected_user_tasks: u64,
) {
    // The current smoke ELF reclaims five ELF pages, four user stack pages,
    // and the final two-page heap. Private mmap pages are unmapped earlier.
    const SMOKE_RECLAIMED_USER_PAGES_PER_TASK: u64 = 11;
    // The current smoke process touches the program, heap/mmap, and stack
    // windows, leaving ten page-table frames to reclaim with the PML4.
    const SMOKE_RECLAIMED_PAGE_TABLE_PAGES_PER_TASK: u64 = 10;
    // User smoke tasks use the current default guarded kernel stack: four
    // writable pages plus one reserved guard page.
    let expected_reclaimed_user_kernel_stack_writable_pages = expected_user_tasks * 4;
    let expected_reclaimed_user_kernel_stack_virtual_pages = expected_user_tasks * 5;
    let expected_reclaimed_user_pages = expected_user_tasks * SMOKE_RECLAIMED_USER_PAGES_PER_TASK;
    let expected_reclaimed_user_page_table_pages =
        expected_user_tasks * SMOKE_RECLAIMED_PAGE_TABLE_PAGES_PER_TASK;
    assert_eq!(
        diagnostics.reclaimed_user_resource_records(),
        expected_user_tasks,
        "finished user tasks must emit one aggregate resource reclaim record"
    );
    assert_eq!(
        diagnostics.reclaimed_user_address_spaces(),
        expected_user_tasks,
        "finished user tasks must reclaim their address spaces"
    );
    assert_eq!(
        diagnostics.reclaimed_user_pages(),
        expected_reclaimed_user_pages,
        "finished user tasks must return user-owned mapped pages"
    );
    assert_eq!(
        diagnostics.reclaimed_user_page_table_pages(),
        expected_reclaimed_user_page_table_pages,
        "finished user tasks must return user page-table pages"
    );
    assert_eq!(
        diagnostics.reclaimed_user_kernel_stacks(),
        expected_user_tasks,
        "finished user tasks must reclaim their kernel stacks"
    );
    assert_eq!(
        diagnostics.reclaimed_user_kernel_stack_writable_pages(),
        expected_reclaimed_user_kernel_stack_writable_pages,
        "finished user tasks must return writable kernel stack pages"
    );
    assert_eq!(
        diagnostics.reclaimed_user_kernel_stack_virtual_pages(),
        expected_reclaimed_user_kernel_stack_virtual_pages,
        "finished user tasks must return guard-inclusive kernel stack virtual pages"
    );
}

fn verify_scheduler_user_return_diagnostics(
    diagnostics: &crate::kernel::task::SchedulerDiagnostics,
    expected_user_tasks: u64,
) {
    let expected_user_stops = expected_user_tasks * 2;
    assert!(
        diagnostics.timer_preemptions() > 0,
        "user smoke must record timer preemption accounting"
    );
    assert!(
        diagnostics.one_shot_user_entries() >= expected_user_tasks,
        "user smoke must enter user tasks through the lifecycle path"
    );
    assert!(
        diagnostics.timer_user_entries() > 0,
        "user smoke must enter at least one user task from timer scheduling"
    );
    assert_eq!(
        diagnostics.user_entries(),
        diagnostics
            .one_shot_user_entries()
            .saturating_add(diagnostics.timer_user_entries()),
        "aggregate user entries must match lifecycle and timer entry counts"
    );
    assert!(
        diagnostics.user_resumes() > 0,
        "user smoke must record user resume accounting"
    );
    assert_eq!(
        diagnostics.pending_user_exits(),
        0,
        "reported user exits must not remain queued after lifecycle cleanup"
    );
    assert!(
        diagnostics.preemption_enabled(),
        "preemption must be re-enabled after active user lifecycle drain"
    );
    assert_eq!(
        diagnostics.preemption_state(),
        crate::kernel::task::PreemptionStateDiagnostics::Enabled,
        "preemption state must be enabled after active user lifecycle drain"
    );
    assert_eq!(
        diagnostics.user_sleep_blocks(),
        expected_user_tasks,
        "every user smoke task must block once in nanosleep"
    );
    assert_eq!(
        diagnostics.user_sleep_wakes(),
        expected_user_tasks,
        "every sleeping user smoke task must wake once"
    );
    assert_eq!(
        diagnostics.user_return_preemption_window_closes(),
        expected_user_stops,
        "every user smoke sleep and exit must close the preemption return window"
    );
    assert_eq!(
        diagnostics.user_return_stack_sets(),
        expected_user_stops,
        "returnable user stacks must be stored once per user stop"
    );
    assert_eq!(
        diagnostics.user_return_stack_takes(),
        expected_user_stops,
        "returnable user stacks must be consumed once per user stop"
    );
}

fn log_scheduler_task_diagnostics(
    diagnostics: &crate::kernel::task::SchedulerDiagnostics,
    states: crate::kernel::task::TaskStateDiagnostics,
) {
    crate::log_info!(
        "task",
        "Scheduler diagnostics verified: total_tasks={} kernel_tasks={} user_tasks={} ready={} running={} blocked={} finished={} active_user_tasks={} active_user_address_spaces={} pending_user_exits={} retained_user_exit_statuses={} waitable_user_exit_statuses={} collected_user_exit_statuses={} preemption_state={} preemption_enabled={} user_sleep_blocks={} user_sleep_wakes={} user_return_preemption_window_closes={} user_return_stack_sets={} user_return_stack_takes={} reclaimed_user_resource_records={} reclaimed_user_address_spaces={} reclaimed_user_pages={} reclaimed_user_page_table_pages={} reclaimed_user_kernel_stacks={} reclaimed_kernel_stack_writable_pages={} reclaimed_kernel_stack_virtual_pages={} context_switches={} timer_preemptions={} user_entries={} one_shot_user_entries={} timer_user_entries={} user_resumes={} finished_tasks={}",
        diagnostics.total_tasks(),
        diagnostics.kernel_tasks(),
        diagnostics.user_tasks(),
        states.ready(),
        states.running(),
        states.blocked(),
        states.finished(),
        diagnostics.active_user_tasks(),
        diagnostics.active_user_address_spaces(),
        diagnostics.pending_user_exits(),
        diagnostics.retained_user_exit_statuses(),
        diagnostics.waitable_user_exit_statuses(),
        diagnostics.collected_user_exit_statuses(),
        diagnostics.preemption_state().as_str(),
        diagnostics.preemption_enabled(),
        diagnostics.user_sleep_blocks(),
        diagnostics.user_sleep_wakes(),
        diagnostics.user_return_preemption_window_closes(),
        diagnostics.user_return_stack_sets(),
        diagnostics.user_return_stack_takes(),
        diagnostics.reclaimed_user_resource_records(),
        diagnostics.reclaimed_user_address_spaces(),
        diagnostics.reclaimed_user_pages(),
        diagnostics.reclaimed_user_page_table_pages(),
        diagnostics.reclaimed_user_kernel_stacks(),
        diagnostics.reclaimed_user_kernel_stack_writable_pages(),
        diagnostics.reclaimed_user_kernel_stack_virtual_pages(),
        diagnostics.context_switches(),
        diagnostics.timer_preemptions(),
        diagnostics.user_entries(),
        diagnostics.one_shot_user_entries(),
        diagnostics.timer_user_entries(),
        diagnostics.user_resumes(),
        diagnostics.finished_tasks()
    );
}

#[derive(Clone, Copy)]
struct UserTaskSnapshotVerification {
    released_mappings: bool,
    fully_reclaimed: bool,
}

/// Verify retained scheduler task snapshots after lifecycle cleanup.
pub fn verify_scheduler_task_snapshots(expected_user_tasks: u64) {
    let snapshots = crate::kernel::task::get_scheduler_task_snapshots()
        .expect("scheduler task snapshots must be available after user smoke tasks");
    let expected_total_tasks = usize::try_from(expected_user_tasks)
        .expect("expected user task count must fit in usize")
        .checked_add(2)
        .expect("expected total task count must not overflow");
    assert_eq!(
        snapshots.len(),
        expected_total_tasks,
        "scheduler task snapshots must include bootstrap, idle, and smoke user tasks"
    );

    let mut finished_user_tasks = 0_u64;
    let mut fully_reclaimed_user_tasks = 0_u64;
    let mut user_vm_snapshots = 0_u64;
    let mut anonymous_mapping_release_snapshots = 0_u64;
    let mut bootstrap_child_user_tasks = 0_u64;
    let mut collected_user_exit_snapshots = 0_u64;
    for snapshot in snapshots {
        if snapshot.kind() != crate::kernel::task::TaskKindDiagnostics::User {
            continue;
        }
        assert!(
            !snapshot.active(),
            "finished user task snapshots must not be active"
        );
        assert_eq!(
            snapshot.state(),
            crate::kernel::task::TaskState::Finished,
            "user smoke task snapshots must be finished"
        );
        assert_eq!(
            snapshot.parent_task_id(),
            Some(crate::kernel::task::TaskIdentifier::BOOTSTRAP.as_u64()),
            "user smoke task snapshots must retain the bootstrap parent task"
        );
        assert_eq!(
            snapshot.exit_code(),
            Some(0),
            "finished user task snapshots must retain exit code zero"
        );
        assert!(
            snapshot.wait_collected(),
            "finished user task snapshots must show collected wait status"
        );
        collected_user_exit_snapshots = collected_user_exit_snapshots.saturating_add(1);
        bootstrap_child_user_tasks = bootstrap_child_user_tasks.saturating_add(1);
        finished_user_tasks = finished_user_tasks.saturating_add(1);
        let verification = verify_user_task_snapshot(snapshot);
        user_vm_snapshots = user_vm_snapshots.saturating_add(1);
        if verification.released_mappings {
            anonymous_mapping_release_snapshots =
                anonymous_mapping_release_snapshots.saturating_add(1);
        }
        if verification.fully_reclaimed {
            fully_reclaimed_user_tasks = fully_reclaimed_user_tasks.saturating_add(1);
        }
    }
    assert_eq!(
        finished_user_tasks, expected_user_tasks,
        "scheduler snapshots must include every finished user smoke task"
    );
    assert_eq!(
        fully_reclaimed_user_tasks, expected_user_tasks,
        "scheduler snapshots must show user task address spaces and kernel stacks reclaimed"
    );
    assert_eq!(
        user_vm_snapshots, expected_user_tasks,
        "scheduler snapshots must include virtual memory bookkeeping for every user task"
    );
    assert_eq!(
        anonymous_mapping_release_snapshots, expected_user_tasks,
        "scheduler snapshots must show anonymous mmap records released"
    );
    assert_eq!(
        bootstrap_child_user_tasks, expected_user_tasks,
        "scheduler snapshots must show every user task as a bootstrap child"
    );
    assert_eq!(
        collected_user_exit_snapshots, expected_user_tasks,
        "scheduler snapshots must show collected user exit statuses"
    );
    crate::log_info!(
        "task",
        "Scheduler task snapshots verified: rows={} finished_user_tasks={} bootstrap_child_user_tasks={} collected_user_exit_snapshots={} fully_reclaimed_user_tasks={} user_vm_snapshots={} released_mmap_snapshots={}",
        expected_total_tasks,
        finished_user_tasks,
        bootstrap_child_user_tasks,
        collected_user_exit_snapshots,
        fully_reclaimed_user_tasks,
        user_vm_snapshots,
        anonymous_mapping_release_snapshots
    );
}

fn verify_user_task_snapshot(
    snapshot: crate::kernel::task::SchedulerTaskSnapshot,
) -> UserTaskSnapshotVerification {
    const SMOKE_PRIVATE_MAPPING_BYTES: u64 = 16_384;
    const SMOKE_TOTAL_PRIVATE_MAPPING_PAGES: u64 = 6;
    const SMOKE_PEAK_PRIVATE_MAPPING_PAGES: u64 = 3;
    const SMOKE_PEAK_PRIVATE_MAPPING_RECORDS: u64 = 2;

    let user_virtual_memory = snapshot
        .user_virtual_memory()
        .expect("user task snapshots must include virtual memory bookkeeping");
    assert_eq!(
        user_virtual_memory.heap_mapped_pages(),
        2,
        "user smoke task snapshots must retain the final two-page brk state"
    );
    assert_eq!(
        user_virtual_memory.mapping_next_start(),
        crate::kernel::memory::user_layout::USER_MAPPING_BASE + SMOKE_PRIVATE_MAPPING_BYTES,
        "user smoke task snapshots must show one three-page anonymous mmap and one file mmap allocation"
    );
    assert_eq!(
        user_virtual_memory.mapping_total_mapped_pages(),
        SMOKE_TOTAL_PRIVATE_MAPPING_PAGES,
        "user smoke task snapshots must retain total private mmap page allocations"
    );
    assert_eq!(
        user_virtual_memory.mapping_total_released_pages(),
        SMOKE_TOTAL_PRIVATE_MAPPING_PAGES,
        "user smoke task snapshots must retain total private mmap page releases"
    );
    assert_eq!(
        user_virtual_memory.mapping_peak_active_pages(),
        SMOKE_PEAK_PRIVATE_MAPPING_PAGES,
        "user smoke task snapshots must retain mmap active-page high-water marks"
    );
    assert_eq!(
        user_virtual_memory.mapping_peak_active_records(),
        SMOKE_PEAK_PRIVATE_MAPPING_RECORDS,
        "user smoke task snapshots must retain mmap record high-water marks"
    );
    assert_eq!(
        user_virtual_memory.mapping_file_private_map_count(),
        1,
        "user smoke task snapshots must retain file-private mmap call counts"
    );

    UserTaskSnapshotVerification {
        released_mappings: user_virtual_memory.mapping_active_pages() == 0
            && user_virtual_memory.mapping_active_records() == 0,
        fully_reclaimed: !snapshot.address_space_owned() && !snapshot.kernel_stack_owned(),
    }
}

/// Record and log a frame allocator diagnostics snapshot.
pub fn record_memory_diagnostics_snapshot(
    frame_allocator: &crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
) {
    crate::kernel::memory::diagnostics::record_frame_allocator_snapshot(frame_allocator);
    let diagnostics = crate::kernel::memory::diagnostics::get_frame_allocator_diagnostics()
        .expect("frame allocator diagnostics must be available after recording a snapshot");
    let owners = diagnostics.owners();
    crate::log_info!(
        "memory",
        "Frame allocator diagnostics snapshot: free={} used={} reserved={} page_table={} kernel_heap={} kernel_stack={} user_stack={} user_elf={} user_heap={} user_mapping={} dynamic_kernel_mapping={} ahci_dma={}",
        diagnostics.free(),
        diagnostics.used(),
        diagnostics.reserved(),
        owners.page_table(),
        owners.kernel_heap(),
        owners.kernel_stack(),
        owners.user_stack(),
        owners.user_elf(),
        owners.user_heap(),
        owners.user_mapping(),
        owners.dynamic_kernel_mapping(),
        owners.ahci_dma()
    );
}

/// Verify the scheduler diagnostics console command.
pub fn verify_scheduler_console_command() {
    match crate::kernel::console::verify_command_smoke_contains(
        "tasks",
        &[
            "reclaimed_user_address_spaces=",
            "process_lifecycle:",
            "collected_user_exit_statuses=",
            "one_shot_user_entries=",
            "timer_user_entries=",
            "user_vm_layout:",
            "task_vm:",
            "task_mmap_lifecycle:",
        ],
    ) {
        Some(output_lines) if output_lines >= 15 => crate::log_info!(
            "console",
            "Tasks command smoke passed: command=\"tasks\" output_lines={}",
            output_lines
        ),
        _ => crate::log_warn!("console", "Tasks command smoke failed: command=\"tasks\""),
    }
}

/// Verify the memory diagnostics console command.
pub fn verify_memory_console_command() {
    match crate::kernel::console::verify_command_smoke("memory") {
        Some(output_lines) if output_lines >= 3 => crate::log_info!(
            "console",
            "Memory command smoke passed: command=\"memory\" output_lines={}",
            output_lines
        ),
        _ => crate::log_warn!("console", "Memory command smoke failed: command=\"memory\""),
    }
}

/// Verify syscall trace console command controls.
pub fn verify_syscall_trace_console_command() {
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
    } else {
        crate::log_warn!(
            "console",
            "Syscall trace controls smoke failed: command=\"syscalls trace\""
        );
    }
}

/// Verify console status strip rendering diagnostics.
pub fn verify_console_status_strip() {
    if crate::kernel::console::verify_status_strip_smoke() {
        crate::log_info!("console", "Console status strip smoke passed.");
    } else {
        crate::log_warn!("console", "Console status strip smoke failed.");
    }
}
