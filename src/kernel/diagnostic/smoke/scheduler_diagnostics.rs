//! Scheduler and frame allocator smoke diagnostics.

use super::{
    USER_SMOKE_CHILD_EXIT_CODE, USER_SMOKE_CHILD_TASK_COUNT, USER_SMOKE_ORPHAN_CHILD_EXIT_CODE,
    USER_SMOKE_PARENT_TASK_COUNT, USER_SMOKE_SHELL_CHILD_TASK_COUNT, USER_SMOKE_SHELL_TASK_COUNT,
};
use crate::kernel::diagnostic::log::{LogField, LogLevel};
use crate::kernel::task::context::UserTrapFrame;

/// Verify scheduler accounting after the userland smoke demo.
pub fn verify_scheduler_task_diagnostics(expected_user_tasks: u64) {
    assert!(
        crate::kernel::task::verify_scheduler_transition_invariants(),
        "scheduler transition invariants must be verifiable after user smoke tasks"
    );
    let diagnostics = crate::kernel::task::get_scheduler_diagnostics()
        .expect("scheduler diagnostics must be available after user smoke tasks");
    let states = diagnostics.states();
    verify_scheduler_task_counts(&diagnostics, states, expected_user_tasks);
    verify_scheduler_reclaim_diagnostics(&diagnostics, expected_user_tasks);
    verify_scheduler_user_return_diagnostics(&diagnostics, expected_user_tasks);
    verify_scheduler_lifecycle_invariants(&diagnostics, expected_user_tasks);
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
        diagnostics.zombie_user_tasks(),
        0,
        "bootstrap parent must leave no zombie user children after wait collection"
    );
    assert_eq!(
        diagnostics.reaped_user_tasks(),
        expected_user_tasks,
        "bootstrap parent must reap every user smoke child"
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
    // The final file_demo image reclaims four ELF pages and four user stack
    // pages. The old heap and private mappings are reclaimed during execve
    // publication, before the task exits.
    const SMOKE_RECLAIMED_USER_PAGES_PER_TASK: u64 = 8;
    // The shell image currently reclaims nine user-owned pages before exit.
    const SHELL_RECLAIMED_USER_PAGES_PER_TASK: u64 = 9;
    // The post-exec image touches only the program and stack windows, leaving
    // seven page-table frames to reclaim with the PML4.
    const SMOKE_RECLAIMED_PAGE_TABLE_PAGES_PER_TASK: u64 = 7;
    // User smoke tasks use the current default guarded kernel stack: four
    // writable pages plus one reserved guard page.
    let expected_shell_tasks =
        u64::try_from(USER_SMOKE_SHELL_TASK_COUNT).expect("shell smoke task count must fit in u64");
    let expected_lifecycle_tasks = expected_user_tasks.saturating_sub(expected_shell_tasks);
    let expected_reclaimed_user_kernel_stack_writable_pages = expected_user_tasks * 4;
    let expected_reclaimed_user_kernel_stack_virtual_pages = expected_user_tasks * 5;
    let expected_reclaimed_user_pages = expected_lifecycle_tasks
        * SMOKE_RECLAIMED_USER_PAGES_PER_TASK
        + expected_shell_tasks * SHELL_RECLAIMED_USER_PAGES_PER_TASK;
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
    assert_eq!(
        diagnostics.address_space_reclaim_guard_checks(),
        expected_user_tasks,
        "finished user address-space reclaim must prove scheduling guard coverage"
    );
    assert_eq!(
        diagnostics.scheduler_transition_invariant_checks(),
        1,
        "user smoke must verify scheduler transition invariants once"
    );
}

fn verify_scheduler_user_return_diagnostics(
    diagnostics: &crate::kernel::task::SchedulerDiagnostics,
    expected_user_tasks: u64,
) {
    assert!(
        diagnostics.timer_preemptions() > 0,
        "user smoke must record timer preemption accounting"
    );
    assert!(
        diagnostics.one_shot_user_entries()
            >= u64::try_from(USER_SMOKE_PARENT_TASK_COUNT)
                .expect("smoke parent task count must fit in u64"),
        "parent smoke tasks must enter through the lifecycle path"
    );
    assert!(
        diagnostics.timer_user_entries() > 0,
        "user smoke must enter at least one user task from timer scheduling"
    );
    assert!(
        diagnostics.timer_user_entries_from_preempted_user() > 0,
        "user smoke must enter a spawned user task from a preempted user task"
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
    assert!(
        diagnostics.user_sleep_blocks()
            >= u64::try_from(USER_SMOKE_PARENT_TASK_COUNT)
                .expect("smoke parent task count must fit in u64"),
        "parent smoke tasks and spawned child waits must block in nanosleep"
    );
    assert_eq!(
        diagnostics.user_sleep_wakes(),
        diagnostics.user_sleep_blocks(),
        "every sleeping user smoke task must wake once"
    );
    let expected_waitpid_blocks = 1 + u64::try_from(USER_SMOKE_SHELL_CHILD_TASK_COUNT)
        .expect("shell child smoke task count must fit in u64");
    assert_eq!(
        diagnostics.user_waitpid_blocks(),
        expected_waitpid_blocks,
        "userland spawn parents must block for waitpid"
    );
    assert_eq!(
        diagnostics.user_waitpid_wakes(),
        diagnostics.user_waitpid_blocks(),
        "every waitpid-blocked user task must wake once"
    );
    assert_eq!(
        diagnostics.user_read_blocks(),
        1,
        "initial user shell must block once on keyboard stdin read"
    );
    assert_eq!(
        diagnostics.user_read_wakes(),
        diagnostics.user_read_blocks(),
        "every read-blocked user task must wake once"
    );
    assert!(
        diagnostics.user_return_preemption_window_closes() >= expected_user_tasks,
        "every user task exit and blocking wait must close the preemption return window"
    );
    assert_eq!(
        diagnostics.user_return_stack_sets(),
        diagnostics.user_return_stack_takes(),
        "returnable user stacks must be stored and consumed in pairs"
    );
    assert_eq!(
        diagnostics.user_return_preemption_window_closes(),
        diagnostics.user_return_stack_sets(),
        "user return window closes must match stored return stacks"
    );
}

fn verify_scheduler_lifecycle_invariants(
    diagnostics: &crate::kernel::task::SchedulerDiagnostics,
    expected_user_tasks: u64,
) {
    let waiting_state_transitions = diagnostics
        .user_sleep_blocks()
        .saturating_add(diagnostics.user_waitpid_blocks())
        .saturating_add(diagnostics.user_read_blocks());
    assert_eq!(
        diagnostics.active_user_tasks(),
        0,
        "lifecycle cleanup must drain the active user task set"
    );
    assert!(
        diagnostics.user_sleep_blocks() > 0,
        "lifecycle smoke must exercise sleep waiting state"
    );
    assert!(
        diagnostics.user_waitpid_blocks() > 0,
        "lifecycle smoke must exercise waitpid waiting state"
    );
    assert!(
        diagnostics.user_read_blocks() > 0,
        "lifecycle smoke must exercise read waiting state"
    );
    assert_eq!(
        diagnostics.zombie_user_tasks(),
        0,
        "lifecycle cleanup must leave no uncollected zombie tasks"
    );
    assert_eq!(
        diagnostics.reaped_user_tasks(),
        expected_user_tasks,
        "lifecycle cleanup must reap every retained user task"
    );
    log_scheduler_lifecycle_invariants(diagnostics, waiting_state_transitions);
}

fn log_scheduler_lifecycle_invariants(
    diagnostics: &crate::kernel::task::SchedulerDiagnostics,
    waiting_state_transitions: u64,
) {
    crate::kernel::diagnostic::log::log_kv(
        LogLevel::Info,
        "task",
        format_args!("Scheduler lifecycle invariants verified"),
        &[
            LogField::new(
                "active_user_tasks",
                format_args!("{}", diagnostics.active_user_tasks()),
            ),
            LogField::new(
                "waiting_state_transitions",
                format_args!("{waiting_state_transitions}"),
            ),
            LogField::new(
                "sleep_waiting_transitions",
                format_args!("{}", diagnostics.user_sleep_blocks()),
            ),
            LogField::new(
                "waitpid_waiting_transitions",
                format_args!("{}", diagnostics.user_waitpid_blocks()),
            ),
            LogField::new(
                "read_waiting_transitions",
                format_args!("{}", diagnostics.user_read_blocks()),
            ),
            LogField::new(
                "zombie_user_tasks",
                format_args!("{}", diagnostics.zombie_user_tasks()),
            ),
            LogField::new(
                "reaped_user_tasks",
                format_args!("{}", diagnostics.reaped_user_tasks()),
            ),
        ],
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
                "zombie_user_tasks",
                format_args!("{}", diagnostics.zombie_user_tasks()),
            ),
            LogField::new(
                "reaped_user_tasks",
                format_args!("{}", diagnostics.reaped_user_tasks()),
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
                "user_waitpid_blocks",
                format_args!("{}", diagnostics.user_waitpid_blocks()),
            ),
            LogField::new(
                "user_waitpid_wakes",
                format_args!("{}", diagnostics.user_waitpid_wakes()),
            ),
            LogField::new(
                "user_read_blocks",
                format_args!("{}", diagnostics.user_read_blocks()),
            ),
            LogField::new(
                "user_read_wakes",
                format_args!("{}", diagnostics.user_read_wakes()),
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
            LogField::new(
                "address_space_reclaim_guard_checks",
                format_args!("{}", diagnostics.address_space_reclaim_guard_checks()),
            ),
            LogField::new(
                "scheduler_transition_invariant_checks",
                format_args!("{}", diagnostics.scheduler_transition_invariant_checks()),
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
                "timer_user_entries_from_preempted_user",
                format_args!("{}", diagnostics.timer_user_entries_from_preempted_user()),
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
    user_image_snapshots: u64,
    published_execve_image_snapshots: u64,
    unreplaced_user_image_snapshots: u64,
    anonymous_mapping_release_snapshots: u64,
    bootstrap_child_user_tasks: u64,
    user_spawned_child_user_tasks: u64,
    collected_user_exit_snapshots: u64,
    reaped_user_task_snapshots: u64,
    runtime_trap_frame_record_snapshots: u64,
    preempted_user_task_snapshots: u64,
    full_timer_trap_frame_snapshots: u64,
    full_restored_trap_frame_snapshots: u64,
    runtime_trap_frame_restore_snapshots: u64,
    resumed_user_task_snapshots: u64,
    resume_handoff_snapshots: u64,
    resume_address_space_root_snapshots: u64,
    resume_kernel_stack_snapshots: u64,
}

impl SchedulerTaskSnapshotCounters {
    const fn new() -> Self {
        Self {
            finished_user_tasks: 0,
            fully_reclaimed_user_tasks: 0,
            user_vm_snapshots: 0,
            user_image_snapshots: 0,
            published_execve_image_snapshots: 0,
            unreplaced_user_image_snapshots: 0,
            anonymous_mapping_release_snapshots: 0,
            bootstrap_child_user_tasks: 0,
            user_spawned_child_user_tasks: 0,
            collected_user_exit_snapshots: 0,
            reaped_user_task_snapshots: 0,
            runtime_trap_frame_record_snapshots: 0,
            preempted_user_task_snapshots: 0,
            full_timer_trap_frame_snapshots: 0,
            full_restored_trap_frame_snapshots: 0,
            runtime_trap_frame_restore_snapshots: 0,
            resumed_user_task_snapshots: 0,
            resume_handoff_snapshots: 0,
            resume_address_space_root_snapshots: 0,
            resume_kernel_stack_snapshots: 0,
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
        record_scheduler_user_task_snapshot(&snapshot, &mut counters);
    }
    counters
}

fn record_scheduler_user_task_snapshot(
    snapshot: &crate::kernel::task::SchedulerTaskSnapshot,
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
    record_parentage_snapshot(snapshot, counters);
    verify_user_task_exit_code(snapshot);
    assert!(
        snapshot.wait_collected(),
        "finished user task snapshots must show collected wait status"
    );
    assert_eq!(
        snapshot.process_lifecycle(),
        crate::kernel::task::TaskProcessLifecycleDiagnostics::Reaped,
        "finished user task snapshots must expose reaped process lifecycle state"
    );
    counters.collected_user_exit_snapshots =
        counters.collected_user_exit_snapshots.saturating_add(1);
    counters.reaped_user_task_snapshots = counters.reaped_user_task_snapshots.saturating_add(1);
    counters.finished_user_tasks = counters.finished_user_tasks.saturating_add(1);
    record_trap_frame_snapshot(snapshot, counters);
    record_resume_handoff_snapshot(snapshot, counters);
    match verify_user_task_image_snapshot(snapshot) {
        crate::kernel::task::UserExecveReplacementStateDiagnostics::Published => {
            counters.published_execve_image_snapshots =
                counters.published_execve_image_snapshots.saturating_add(1);
        }
        crate::kernel::task::UserExecveReplacementStateDiagnostics::None => {
            counters.unreplaced_user_image_snapshots =
                counters.unreplaced_user_image_snapshots.saturating_add(1);
        }
        crate::kernel::task::UserExecveReplacementStateDiagnostics::CandidateDropped => {
            panic!("user smoke snapshots must not report dropped execve candidates");
        }
    }
    counters.user_image_snapshots = counters.user_image_snapshots.saturating_add(1);
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

fn record_trap_frame_snapshot(
    snapshot: &crate::kernel::task::SchedulerTaskSnapshot,
    counters: &mut SchedulerTaskSnapshotCounters,
) {
    if snapshot.syscall_frame_recorded() || snapshot.interrupt_frame_recorded() {
        assert!(
            snapshot.runtime_trap_frame_record_count() > 0,
            "recorded syscall or timer frames must pass through the unified scheduler trap-frame path"
        );
        counters.runtime_trap_frame_record_snapshots = counters
            .runtime_trap_frame_record_snapshots
            .saturating_add(1);
    }
    if snapshot.last_preemption_reason()
        == crate::kernel::task::UserPreemptionReasonDiagnostics::Timer
    {
        assert!(
            snapshot.interrupt_frame_recorded(),
            "timer-preempted user task snapshots must retain an interrupt trap frame"
        );
        assert_eq!(
            snapshot.saved_user_trap_frame_bytes(),
            core::mem::size_of::<UserTrapFrame>(),
            "timer-preempted user task snapshots must retain a complete user trap frame"
        );
        counters.preempted_user_task_snapshots =
            counters.preempted_user_task_snapshots.saturating_add(1);
        counters.full_timer_trap_frame_snapshots =
            counters.full_timer_trap_frame_snapshots.saturating_add(1);
        assert!(
            snapshot.runtime_trap_frame_restore_count() > 0,
            "timer-preempted user task snapshots must show a runtime trap frame restore"
        );
    }
    assert_eq!(
        snapshot.restored_user_trap_frame_bytes(),
        core::mem::size_of::<UserTrapFrame>(),
        "finished user task snapshots must retain a complete restored user trap frame"
    );
    counters.full_restored_trap_frame_snapshots = counters
        .full_restored_trap_frame_snapshots
        .saturating_add(1);
    if snapshot.runtime_trap_frame_restore_count() > 0 {
        counters.runtime_trap_frame_restore_snapshots = counters
            .runtime_trap_frame_restore_snapshots
            .saturating_add(1);
    }
    counters.resumed_user_task_snapshots = counters.resumed_user_task_snapshots.saturating_add(1);
}

fn record_resume_handoff_snapshot(
    snapshot: &crate::kernel::task::SchedulerTaskSnapshot,
    counters: &mut SchedulerTaskSnapshotCounters,
) {
    assert_ne!(
        snapshot.last_resume_path(),
        crate::kernel::task::UserResumePathDiagnostics::None,
        "finished user task snapshots must retain their last user resume path"
    );
    assert!(
        snapshot.resume_handoff_count() > 0,
        "finished user task snapshots must retain a scheduler resume handoff"
    );
    assert_ne!(
        snapshot.last_resume_address_space_root(),
        0,
        "finished user task snapshots must retain their last resume address-space root"
    );
    assert_ne!(
        snapshot.last_resume_kernel_stack_top(),
        0,
        "finished user task snapshots must retain their last resume kernel stack top"
    );
    counters.resume_handoff_snapshots = counters.resume_handoff_snapshots.saturating_add(1);
    counters.resume_address_space_root_snapshots = counters
        .resume_address_space_root_snapshots
        .saturating_add(1);
    counters.resume_kernel_stack_snapshots =
        counters.resume_kernel_stack_snapshots.saturating_add(1);
}

fn verify_user_task_exit_code(snapshot: &crate::kernel::task::SchedulerTaskSnapshot) {
    let exit_code = snapshot
        .exit_code()
        .expect("finished user task snapshots must retain their exit code");
    if snapshot.parent_task_id() == Some(crate::kernel::task::TaskIdentifier::BOOTSTRAP.as_u64()) {
        assert!(
            exit_code == 0 || exit_code == USER_SMOKE_ORPHAN_CHILD_EXIT_CODE,
            "bootstrap-owned smoke tasks must retain zero or reparented orphan exit status"
        );
        return;
    }
    assert!(
        exit_code == 0
            || exit_code == USER_SMOKE_CHILD_EXIT_CODE
            || exit_code == USER_SMOKE_ORPHAN_CHILD_EXIT_CODE,
        "user-spawned child tasks must retain known smoke exit status"
    );
}

fn record_parentage_snapshot(
    snapshot: &crate::kernel::task::SchedulerTaskSnapshot,
    counters: &mut SchedulerTaskSnapshotCounters,
) {
    if snapshot.parent_task_id() == Some(crate::kernel::task::TaskIdentifier::BOOTSTRAP.as_u64()) {
        counters.bootstrap_child_user_tasks = counters.bootstrap_child_user_tasks.saturating_add(1);
    } else {
        assert!(
            snapshot.parent_task_id().is_some(),
            "user-spawned child snapshots must retain a user parent task"
        );
        counters.user_spawned_child_user_tasks =
            counters.user_spawned_child_user_tasks.saturating_add(1);
    }
}

fn verify_scheduler_task_snapshot_counts(
    counters: SchedulerTaskSnapshotCounters,
    expected_user_tasks: u64,
) {
    // Two bootstrap children start file_demo directly for spawn/wait and
    // parent-exit marker paths, so they have no execve replacement history.
    const DIRECT_FILE_DEMO_PARENT_TASKS: u64 = 2;
    let expected_reparented_orphan_children = 1_u64;
    let expected_shell_tasks =
        u64::try_from(USER_SMOKE_SHELL_TASK_COUNT).expect("shell smoke task count must fit in u64");
    let expected_shell_child_tasks = u64::try_from(USER_SMOKE_SHELL_CHILD_TASK_COUNT)
        .expect("shell child smoke task count must fit in u64");
    let expected_bootstrap_parent_tasks = u64::try_from(USER_SMOKE_PARENT_TASK_COUNT)
        .expect("smoke parent task count must fit in u64");
    let expected_bootstrap_children = expected_bootstrap_parent_tasks
        .saturating_add(expected_reparented_orphan_children)
        .saturating_add(expected_shell_tasks);
    let expected_user_spawned_children = u64::try_from(USER_SMOKE_CHILD_TASK_COUNT)
        .expect("smoke child task count must fit in u64")
        .saturating_sub(expected_reparented_orphan_children)
        .saturating_add(expected_shell_child_tasks);
    let expected_published_execve_images =
        expected_bootstrap_parent_tasks.saturating_sub(DIRECT_FILE_DEMO_PARENT_TASKS);
    let expected_unreplaced_user_images = u64::try_from(USER_SMOKE_CHILD_TASK_COUNT)
        .expect("smoke child task count must fit in u64")
        .saturating_add(DIRECT_FILE_DEMO_PARENT_TASKS)
        .saturating_add(expected_shell_tasks)
        .saturating_add(expected_shell_child_tasks);
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
        counters.user_image_snapshots, expected_user_tasks,
        "scheduler snapshots must include execve image diagnostics for every user task"
    );
    assert_eq!(
        counters.published_execve_image_snapshots, expected_published_execve_images,
        "scheduler snapshots must show smoke execve parents published replacements"
    );
    assert_eq!(
        counters.unreplaced_user_image_snapshots,
        expected_unreplaced_user_images,
        "scheduler snapshots must show directly spawned file_demo tasks have no execve replacement history"
    );
    assert_eq!(
        counters.anonymous_mapping_release_snapshots, expected_user_tasks,
        "scheduler snapshots must show anonymous mmap records released"
    );
    assert_eq!(
        counters.bootstrap_child_user_tasks, expected_bootstrap_children,
        "scheduler snapshots must show smoke parents and reparented orphan children as bootstrap children"
    );
    assert_eq!(
        counters.user_spawned_child_user_tasks, expected_user_spawned_children,
        "scheduler snapshots must show unreparented user-spawned child tasks"
    );
    assert_eq!(
        counters.collected_user_exit_snapshots, expected_user_tasks,
        "scheduler snapshots must show collected user exit statuses"
    );
    assert_eq!(
        counters.reaped_user_task_snapshots, expected_user_tasks,
        "scheduler snapshots must show reaped user process lifecycle states"
    );
    assert_eq!(
        counters.runtime_trap_frame_record_snapshots, expected_user_tasks,
        "scheduler snapshots must show every user smoke task used the unified trap-frame record path"
    );
    assert!(
        counters.preempted_user_task_snapshots > 0,
        "scheduler snapshots must show at least one timer-preempted user task"
    );
    assert_eq!(
        counters.full_timer_trap_frame_snapshots, counters.preempted_user_task_snapshots,
        "every timer-preempted user task snapshot must retain a complete interrupt trap frame"
    );
    assert_eq!(
        counters.full_restored_trap_frame_snapshots, expected_user_tasks,
        "scheduler snapshots must show every finished user task restored a complete user trap frame"
    );
    assert!(
        counters.runtime_trap_frame_restore_snapshots >= counters.preempted_user_task_snapshots,
        "every timer-preempted user task snapshot must show a runtime trap frame restore"
    );
    assert_eq!(
        counters.resumed_user_task_snapshots, expected_user_tasks,
        "scheduler snapshots must show every user task was entered or resumed"
    );
    verify_resume_handoff_snapshot_counts(counters, expected_user_tasks);
}

fn verify_resume_handoff_snapshot_counts(
    counters: SchedulerTaskSnapshotCounters,
    expected_user_tasks: u64,
) {
    assert_eq!(
        counters.resume_handoff_snapshots, expected_user_tasks,
        "scheduler snapshots must show every user task retained a resume handoff"
    );
    assert_eq!(
        counters.resume_address_space_root_snapshots, expected_user_tasks,
        "scheduler snapshots must show every user task retained a resume address-space root"
    );
    assert_eq!(
        counters.resume_kernel_stack_snapshots, expected_user_tasks,
        "scheduler snapshots must show every user task retained a resume kernel stack top"
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
                "user_spawned_child_user_tasks",
                format_args!("{}", counters.user_spawned_child_user_tasks),
            ),
            LogField::new(
                "collected_user_exit_snapshots",
                format_args!("{}", counters.collected_user_exit_snapshots),
            ),
            LogField::new(
                "reaped_user_task_snapshots",
                format_args!("{}", counters.reaped_user_task_snapshots),
            ),
            LogField::new(
                "runtime_trap_frame_record_snapshots",
                format_args!("{}", counters.runtime_trap_frame_record_snapshots),
            ),
            LogField::new(
                "preempted_user_task_snapshots",
                format_args!("{}", counters.preempted_user_task_snapshots),
            ),
            LogField::new(
                "full_timer_trap_frame_snapshots",
                format_args!("{}", counters.full_timer_trap_frame_snapshots),
            ),
            LogField::new(
                "full_restored_trap_frame_snapshots",
                format_args!("{}", counters.full_restored_trap_frame_snapshots),
            ),
            LogField::new(
                "runtime_trap_frame_restore_snapshots",
                format_args!("{}", counters.runtime_trap_frame_restore_snapshots),
            ),
            LogField::new(
                "resumed_user_task_snapshots",
                format_args!("{}", counters.resumed_user_task_snapshots),
            ),
            LogField::new(
                "resume_handoff_snapshots",
                format_args!("{}", counters.resume_handoff_snapshots),
            ),
            LogField::new(
                "resume_address_space_root_snapshots",
                format_args!("{}", counters.resume_address_space_root_snapshots),
            ),
            LogField::new(
                "resume_kernel_stack_snapshots",
                format_args!("{}", counters.resume_kernel_stack_snapshots),
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
                "user_image_snapshots",
                format_args!("{}", counters.user_image_snapshots),
            ),
            LogField::new(
                "published_execve_image_snapshots",
                format_args!("{}", counters.published_execve_image_snapshots),
            ),
            LogField::new(
                "unreplaced_user_image_snapshots",
                format_args!("{}", counters.unreplaced_user_image_snapshots),
            ),
            LogField::new(
                "released_mmap_snapshots",
                format_args!("{}", counters.anonymous_mapping_release_snapshots),
            ),
        ],
    );
}

fn verify_user_task_snapshot(
    snapshot: &crate::kernel::task::SchedulerTaskSnapshot,
) -> UserTaskSnapshotVerification {
    let user_virtual_memory = snapshot
        .user_virtual_memory()
        .expect("user task snapshots must include virtual memory bookkeeping");
    assert_eq!(
        user_virtual_memory.heap_mapped_pages(),
        0,
        "execve must reset user heap bookkeeping before task exit"
    );
    assert_eq!(
        user_virtual_memory.mapping_next_start(),
        crate::kernel::memory::user_layout::USER_MAPPING_BASE,
        "execve must reset private mapping placement bookkeeping before task exit"
    );
    assert_eq!(
        user_virtual_memory.mapping_total_mapped_pages(),
        0,
        "execve must reset total private mapping allocations"
    );
    assert_eq!(
        user_virtual_memory.mapping_total_released_pages(),
        0,
        "execve must reset total private mapping releases"
    );
    assert_eq!(
        user_virtual_memory.mapping_peak_active_pages(),
        0,
        "execve must reset private mapping active-page high-water marks"
    );
    assert_eq!(
        user_virtual_memory.mapping_peak_active_records(),
        0,
        "execve must reset private mapping record high-water marks"
    );
    assert_eq!(
        user_virtual_memory.mapping_file_private_map_count(),
        0,
        "execve must reset file-private mmap call counts"
    );

    UserTaskSnapshotVerification {
        released_mappings: user_virtual_memory.mapping_active_pages() == 0
            && user_virtual_memory.mapping_active_records() == 0,
        fully_reclaimed: !snapshot.address_space_owned() && !snapshot.kernel_stack_owned(),
    }
}

fn verify_user_task_image_snapshot(
    snapshot: &crate::kernel::task::SchedulerTaskSnapshot,
) -> crate::kernel::task::UserExecveReplacementStateDiagnostics {
    let user_image = snapshot
        .user_image()
        .expect("user task snapshots must include image diagnostics");
    if user_image_origin_matches(user_image, b"/disk/bin/smoke_demo") {
        verify_smoke_parent_image_snapshot(user_image);
        crate::kernel::task::UserExecveReplacementStateDiagnostics::Published
    } else if user_image_origin_matches(user_image, b"/disk/bin/file_demo") {
        verify_direct_spawn_image_snapshot(user_image, b"/disk/bin/file_demo");
        crate::kernel::task::UserExecveReplacementStateDiagnostics::None
    } else if user_image_origin_matches(user_image, b"/disk/bin/user_shell") {
        verify_direct_spawn_image_snapshot(user_image, b"/disk/bin/user_shell");
        crate::kernel::task::UserExecveReplacementStateDiagnostics::None
    } else {
        panic!("user task image snapshot must retain a known smoke origin path");
    }
}

fn verify_smoke_parent_image_snapshot(
    user_image: &crate::kernel::task::UserImageDiagnosticsSnapshot,
) {
    assert_eq!(
        user_image.last_execve_state(),
        crate::kernel::task::UserExecveReplacementStateDiagnostics::Published,
        "user smoke parent tasks must report a published execve replacement state"
    );
    assert_eq!(
        user_image.generation(),
        2,
        "user smoke parent tasks must record two successful execve generations"
    );
    assert_user_image_path(
        user_image,
        b"/disk/bin/file_demo",
        "user smoke parent tasks must record the post-exec image path",
    );
    assert_eq!(
        user_image.last_execve_old_user_pages(),
        9,
        "execve diagnostics must record old user page reclaim count"
    );
    assert_eq!(
        user_image.last_execve_old_page_table_pages(),
        7,
        "execve diagnostics must record old page-table reclaim count"
    );
}

fn verify_direct_spawn_image_snapshot(
    user_image: &crate::kernel::task::UserImageDiagnosticsSnapshot,
    expected_path: &[u8],
) {
    assert_eq!(
        user_image.last_execve_state(),
        crate::kernel::task::UserExecveReplacementStateDiagnostics::None,
        "directly spawned user tasks must not report an execve replacement state"
    );
    assert_eq!(
        user_image.generation(),
        0,
        "directly spawned user tasks must not report an execve generation"
    );
    assert_user_image_path(
        user_image,
        expected_path,
        "directly spawned user tasks must retain their spawn image path",
    );
    assert_eq!(
        user_image.last_execve_old_user_pages(),
        0,
        "directly spawned user tasks must not report execve reclaim pages"
    );
    assert_eq!(
        user_image.last_execve_old_page_table_pages(),
        0,
        "directly spawned user tasks must not report execve page-table reclaim"
    );
}

fn user_image_origin_matches(
    user_image: &crate::kernel::task::UserImageDiagnosticsSnapshot,
    expected_path: &[u8],
) -> bool {
    let origin_path_bytes = user_image.origin_path_bytes();
    &origin_path_bytes[..user_image.origin_path_len()] == expected_path
}

fn assert_user_image_path(
    user_image: &crate::kernel::task::UserImageDiagnosticsSnapshot,
    expected_path: &[u8],
    message: &str,
) {
    let path_bytes = user_image.path_bytes();
    assert_eq!(
        &path_bytes[..user_image.path_len()],
        expected_path,
        "{message}"
    );
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
