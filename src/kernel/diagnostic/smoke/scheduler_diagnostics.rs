//! Scheduler and frame allocator smoke diagnostics.

use crate::kernel::diagnostic::log::{LogField, LogLevel};

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
    crate::kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "task",
        format_args!("Scheduler diagnostics verified"),
        &[
            LogField::new("total_tasks", format_args!("{}", diagnostics.total_tasks())),
            LogField::new(
                "kernel_tasks",
                format_args!("{}", diagnostics.kernel_tasks()),
            ),
            LogField::new("user_tasks", format_args!("{}", diagnostics.user_tasks())),
            LogField::new("ready", format_args!("{}", states.ready())),
            LogField::new("running", format_args!("{}", states.running())),
            LogField::new("blocked", format_args!("{}", states.blocked())),
            LogField::new("finished", format_args!("{}", states.finished())),
            LogField::new(
                "active_user_tasks",
                format_args!("{}", diagnostics.active_user_tasks()),
            ),
            LogField::new(
                "active_user_address_spaces",
                format_args!("{}", diagnostics.active_user_address_spaces()),
            ),
            LogField::new(
                "pending_user_exits",
                format_args!("{}", diagnostics.pending_user_exits()),
            ),
            LogField::new(
                "retained_user_exit_statuses",
                format_args!("{}", diagnostics.retained_user_exit_statuses()),
            ),
            LogField::new(
                "waitable_user_exit_statuses",
                format_args!("{}", diagnostics.waitable_user_exit_statuses()),
            ),
            LogField::new(
                "collected_user_exit_statuses",
                format_args!("{}", diagnostics.collected_user_exit_statuses()),
            ),
            LogField::new(
                "preemption_state",
                format_args!("{}", diagnostics.preemption_state().as_str()),
            ),
            LogField::new(
                "preemption_enabled",
                format_args!("{}", diagnostics.preemption_enabled()),
            ),
            LogField::new(
                "user_sleep_blocks",
                format_args!("{}", diagnostics.user_sleep_blocks()),
            ),
            LogField::new(
                "user_sleep_wakes",
                format_args!("{}", diagnostics.user_sleep_wakes()),
            ),
            LogField::new(
                "user_return_preemption_window_closes",
                format_args!("{}", diagnostics.user_return_preemption_window_closes()),
            ),
            LogField::new(
                "user_return_stack_sets",
                format_args!("{}", diagnostics.user_return_stack_sets()),
            ),
            LogField::new(
                "user_return_stack_takes",
                format_args!("{}", diagnostics.user_return_stack_takes()),
            ),
        ],
    );
    log_scheduler_reclaim_diagnostics(diagnostics);
    log_scheduler_switch_diagnostics(diagnostics);
}

fn log_scheduler_reclaim_diagnostics(diagnostics: &crate::kernel::task::SchedulerDiagnostics) {
    crate::kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "task",
        format_args!("Scheduler reclaim diagnostics verified"),
        &[
            LogField::new(
                "reclaimed_user_resource_records",
                format_args!("{}", diagnostics.reclaimed_user_resource_records()),
            ),
            LogField::new(
                "reclaimed_user_address_spaces",
                format_args!("{}", diagnostics.reclaimed_user_address_spaces()),
            ),
            LogField::new(
                "reclaimed_user_pages",
                format_args!("{}", diagnostics.reclaimed_user_pages()),
            ),
            LogField::new(
                "reclaimed_user_page_table_pages",
                format_args!("{}", diagnostics.reclaimed_user_page_table_pages()),
            ),
            LogField::new(
                "reclaimed_user_kernel_stacks",
                format_args!("{}", diagnostics.reclaimed_user_kernel_stacks()),
            ),
            LogField::new(
                "reclaimed_kernel_stack_writable_pages",
                format_args!(
                    "{}",
                    diagnostics.reclaimed_user_kernel_stack_writable_pages()
                ),
            ),
            LogField::new(
                "reclaimed_kernel_stack_virtual_pages",
                format_args!(
                    "{}",
                    diagnostics.reclaimed_user_kernel_stack_virtual_pages()
                ),
            ),
        ],
    );
}

fn log_scheduler_switch_diagnostics(diagnostics: &crate::kernel::task::SchedulerDiagnostics) {
    crate::kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "task",
        format_args!("Scheduler switch diagnostics verified"),
        &[
            LogField::new(
                "context_switches",
                format_args!("{}", diagnostics.context_switches()),
            ),
            LogField::new(
                "timer_preemptions",
                format_args!("{}", diagnostics.timer_preemptions()),
            ),
            LogField::new(
                "user_entries",
                format_args!("{}", diagnostics.user_entries()),
            ),
            LogField::new(
                "one_shot_user_entries",
                format_args!("{}", diagnostics.one_shot_user_entries()),
            ),
            LogField::new(
                "timer_user_entries",
                format_args!("{}", diagnostics.timer_user_entries()),
            ),
            LogField::new(
                "user_resumes",
                format_args!("{}", diagnostics.user_resumes()),
            ),
            LogField::new(
                "finished_tasks",
                format_args!("{}", diagnostics.finished_tasks()),
            ),
        ],
    );
}

#[derive(Clone, Copy)]
struct UserTaskSnapshotVerification {
    released_mappings: bool,
    fully_reclaimed: bool,
}

#[derive(Clone, Copy)]
struct SchedulerTaskSnapshotCounters {
    finished_user_tasks: u64,
    fully_reclaimed_user_tasks: u64,
    user_vm_snapshots: u64,
    anonymous_mapping_release_snapshots: u64,
    bootstrap_child_user_tasks: u64,
    collected_user_exit_snapshots: u64,
}

impl SchedulerTaskSnapshotCounters {
    const fn new() -> Self {
        Self {
            finished_user_tasks: 0,
            fully_reclaimed_user_tasks: 0,
            user_vm_snapshots: 0,
            anonymous_mapping_release_snapshots: 0,
            bootstrap_child_user_tasks: 0,
            collected_user_exit_snapshots: 0,
        }
    }
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

    let counters = verify_scheduler_task_snapshot_rows(snapshots);
    verify_scheduler_task_snapshot_counts(counters, expected_user_tasks);
    log_scheduler_task_snapshot_counters(counters, expected_total_tasks);
}

fn verify_scheduler_task_snapshot_rows(
    snapshots: alloc::vec::Vec<crate::kernel::task::SchedulerTaskSnapshot>,
) -> SchedulerTaskSnapshotCounters {
    let mut counters = SchedulerTaskSnapshotCounters::new();
    for snapshot in snapshots {
        if snapshot.kind() != crate::kernel::task::TaskKindDiagnostics::User {
            continue;
        }
        record_scheduler_user_task_snapshot(snapshot, &mut counters);
    }
    counters
}

fn record_scheduler_user_task_snapshot(
    snapshot: crate::kernel::task::SchedulerTaskSnapshot,
    counters: &mut SchedulerTaskSnapshotCounters,
) {
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
    counters.collected_user_exit_snapshots =
        counters.collected_user_exit_snapshots.saturating_add(1);
    counters.bootstrap_child_user_tasks = counters.bootstrap_child_user_tasks.saturating_add(1);
    counters.finished_user_tasks = counters.finished_user_tasks.saturating_add(1);
    let verification = verify_user_task_snapshot(snapshot);
    counters.user_vm_snapshots = counters.user_vm_snapshots.saturating_add(1);
    if verification.released_mappings {
        counters.anonymous_mapping_release_snapshots = counters
            .anonymous_mapping_release_snapshots
            .saturating_add(1);
    }
    if verification.fully_reclaimed {
        counters.fully_reclaimed_user_tasks = counters.fully_reclaimed_user_tasks.saturating_add(1);
    }
}

fn verify_scheduler_task_snapshot_counts(
    counters: SchedulerTaskSnapshotCounters,
    expected_user_tasks: u64,
) {
    assert_eq!(
        counters.finished_user_tasks, expected_user_tasks,
        "scheduler snapshots must include every finished user smoke task"
    );
    assert_eq!(
        counters.fully_reclaimed_user_tasks, expected_user_tasks,
        "scheduler snapshots must show user task address spaces and kernel stacks reclaimed"
    );
    assert_eq!(
        counters.user_vm_snapshots, expected_user_tasks,
        "scheduler snapshots must include virtual memory bookkeeping for every user task"
    );
    assert_eq!(
        counters.anonymous_mapping_release_snapshots, expected_user_tasks,
        "scheduler snapshots must show anonymous mmap records released"
    );
    assert_eq!(
        counters.bootstrap_child_user_tasks, expected_user_tasks,
        "scheduler snapshots must show every user task as a bootstrap child"
    );
    assert_eq!(
        counters.collected_user_exit_snapshots, expected_user_tasks,
        "scheduler snapshots must show collected user exit statuses"
    );
}

fn log_scheduler_task_snapshot_counters(
    counters: SchedulerTaskSnapshotCounters,
    expected_total_tasks: usize,
) {
    crate::kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "task",
        format_args!("Scheduler task snapshots verified"),
        &[
            LogField::new("rows", format_args!("{expected_total_tasks}")),
            LogField::new(
                "finished_user_tasks",
                format_args!("{}", counters.finished_user_tasks),
            ),
            LogField::new(
                "bootstrap_child_user_tasks",
                format_args!("{}", counters.bootstrap_child_user_tasks),
            ),
            LogField::new(
                "collected_user_exit_snapshots",
                format_args!("{}", counters.collected_user_exit_snapshots),
            ),
            LogField::new(
                "fully_reclaimed_user_tasks",
                format_args!("{}", counters.fully_reclaimed_user_tasks),
            ),
            LogField::new(
                "user_vm_snapshots",
                format_args!("{}", counters.user_vm_snapshots),
            ),
            LogField::new(
                "released_mmap_snapshots",
                format_args!("{}", counters.anonymous_mapping_release_snapshots),
            ),
        ],
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
    crate::kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "memory",
        format_args!("Frame allocator diagnostics snapshot"),
        &[
            LogField::new("free", format_args!("{}", diagnostics.free())),
            LogField::new("used", format_args!("{}", diagnostics.used())),
            LogField::new("reserved", format_args!("{}", diagnostics.reserved())),
            LogField::new("page_table", format_args!("{}", owners.page_table())),
            LogField::new("kernel_heap", format_args!("{}", owners.kernel_heap())),
            LogField::new("kernel_stack", format_args!("{}", owners.kernel_stack())),
            LogField::new("user_stack", format_args!("{}", owners.user_stack())),
            LogField::new("user_elf", format_args!("{}", owners.user_elf())),
            LogField::new("user_heap", format_args!("{}", owners.user_heap())),
            LogField::new("user_mapping", format_args!("{}", owners.user_mapping())),
            LogField::new(
                "dynamic_kernel_mapping",
                format_args!("{}", owners.dynamic_kernel_mapping()),
            ),
            LogField::new("ahci_dma", format_args!("{}", owners.ahci_dma())),
        ],
    );
}
