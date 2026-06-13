//! Scheduler diagnostics snapshots.

use super::state::TaskState;

/// Number of tasks currently in each lifecycle state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TaskStateDiagnostics {
    ready: u64,
    running: u64,
    blocked: u64,
    finished: u64,
}

impl TaskStateDiagnostics {
    /// Create task state diagnostics from individual state counts.
    pub(super) const fn new(ready: u64, running: u64, blocked: u64, finished: u64) -> Self {
        Self {
            ready,
            running,
            blocked,
            finished,
        }
    }

    /// Return the number of ready tasks.
    pub const fn ready(self) -> u64 {
        self.ready
    }

    /// Return the number of running tasks.
    pub const fn running(self) -> u64 {
        self.running
    }

    /// Return the number of blocked tasks.
    pub const fn blocked(self) -> u64 {
        self.blocked
    }

    /// Return the number of finished tasks.
    pub const fn finished(self) -> u64 {
        self.finished
    }
}

/// Schedulable task kind reported by scheduler diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskKindDiagnostics {
    /// A kernel-mode task.
    Kernel,
    /// A user-mode task.
    User,
}

impl TaskKindDiagnostics {
    /// Return a stable label for console diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kernel => "kernel",
            Self::User => "user",
        }
    }
}

/// Snapshot of one scheduler task record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerTaskSnapshot {
    task_id: u64,
    parent_task_id: Option<u64>,
    kind: TaskKindDiagnostics,
    state: TaskState,
    active: bool,
    address_space_owned: bool,
    kernel_stack_owned: bool,
}

impl SchedulerTaskSnapshot {
    /// Create a task snapshot from scheduler-owned task metadata.
    pub(super) const fn new(
        task_id: u64,
        parent_task_id: Option<u64>,
        kind: TaskKindDiagnostics,
        state: TaskState,
        active: bool,
        address_space_owned: bool,
        kernel_stack_owned: bool,
    ) -> Self {
        Self {
            task_id,
            parent_task_id,
            kind,
            state,
            active,
            address_space_owned,
            kernel_stack_owned,
        }
    }

    /// Return the scheduler-local task identifier.
    pub const fn task_id(self) -> u64 {
        self.task_id
    }

    /// Return the parent task identifier if the task was spawned by another task.
    pub const fn parent_task_id(self) -> Option<u64> {
        self.parent_task_id
    }

    /// Return the scheduler task kind.
    pub const fn kind(self) -> TaskKindDiagnostics {
        self.kind
    }

    /// Return the current scheduler lifecycle state.
    pub const fn state(self) -> TaskState {
        self.state
    }

    /// Return whether this user task is in the active scheduling set.
    pub const fn active(self) -> bool {
        self.active
    }

    /// Return whether this task still owns a user address space.
    pub const fn address_space_owned(self) -> bool {
        self.address_space_owned
    }

    /// Return whether this task still owns a scheduler-managed kernel stack.
    pub const fn kernel_stack_owned(self) -> bool {
        self.kernel_stack_owned
    }
}

/// Snapshot of scheduler task counts and lifecycle accounting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SchedulerDiagnostics {
    pub(super) total_tasks: u64,
    pub(super) kernel_tasks: u64,
    pub(super) user_tasks: u64,
    pub(super) active_user_tasks: u64,
    pub(super) active_user_address_spaces: u64,
    pub(super) states: TaskStateDiagnostics,
    pub(super) context_switches: u64,
    pub(super) timer_preemptions: u64,
    pub(super) user_entries: u64,
    pub(super) user_resumes: u64,
    pub(super) finished_tasks: u64,
    pub(super) pending_user_exits: u64,
    pub(super) preemption_enabled: bool,
    pub(super) user_exit_preemption_window_closes: u64,
    pub(super) user_exit_return_stack_sets: u64,
    pub(super) user_exit_return_stack_takes: u64,
    pub(super) reclaimed_user_resource_records: u64,
    pub(super) reclaimed_user_kernel_stacks: u64,
    pub(super) reclaimed_user_kernel_stack_writable_pages: u64,
    pub(super) reclaimed_user_kernel_stack_virtual_pages: u64,
}

impl SchedulerDiagnostics {
    /// Return the total number of task records retained by the scheduler.
    pub const fn total_tasks(self) -> u64 {
        self.total_tasks
    }

    /// Return the number of kernel task records.
    pub const fn kernel_tasks(self) -> u64 {
        self.kernel_tasks
    }

    /// Return the number of user task records.
    pub const fn user_tasks(self) -> u64 {
        self.user_tasks
    }

    /// Return the number of user tasks in the active scheduling set.
    pub const fn active_user_tasks(self) -> u64 {
        self.active_user_tasks
    }

    /// Return the number of user tasks that still own an address space.
    pub const fn active_user_address_spaces(self) -> u64 {
        self.active_user_address_spaces
    }

    /// Return the lifecycle state counts for all task records.
    pub const fn states(self) -> TaskStateDiagnostics {
        self.states
    }

    /// Return the number of timer-driven scheduler context switches.
    pub const fn context_switches(self) -> u64 {
        self.context_switches
    }

    /// Return the number of timer preemptions from user mode.
    pub const fn timer_preemptions(self) -> u64 {
        self.timer_preemptions
    }

    /// Return the number of entries into a user task.
    pub const fn user_entries(self) -> u64 {
        self.user_entries
    }

    /// Return the number of resumes into an already-started user task.
    pub const fn user_resumes(self) -> u64 {
        self.user_resumes
    }

    /// Return the number of tasks marked finished through the scheduler.
    pub const fn finished_tasks(self) -> u64 {
        self.finished_tasks
    }

    /// Return the number of finished user exits waiting to be reported.
    pub const fn pending_user_exits(self) -> u64 {
        self.pending_user_exits
    }

    /// Return whether timer-driven task switching is currently enabled.
    pub const fn preemption_enabled(self) -> bool {
        self.preemption_enabled
    }

    /// Return the number of user exits that closed the scheduler preemption window.
    pub const fn user_exit_preemption_window_closes(self) -> u64 {
        self.user_exit_preemption_window_closes
    }

    /// Return the number of one-shot user exit return stacks stored before Ring 3 entry.
    pub const fn user_exit_return_stack_sets(self) -> u64 {
        self.user_exit_return_stack_sets
    }

    /// Return the number of one-shot user exit return stacks consumed by `SYS_EXIT`.
    pub const fn user_exit_return_stack_takes(self) -> u64 {
        self.user_exit_return_stack_takes
    }

    /// Return the number of finished user task resource reclaim records.
    pub const fn reclaimed_user_resource_records(self) -> u64 {
        self.reclaimed_user_resource_records
    }

    /// Return the number of finished user task kernel stacks reclaimed.
    pub const fn reclaimed_user_kernel_stacks(self) -> u64 {
        self.reclaimed_user_kernel_stacks
    }

    /// Return the number of writable kernel stack pages reclaimed from user tasks.
    pub const fn reclaimed_user_kernel_stack_writable_pages(self) -> u64 {
        self.reclaimed_user_kernel_stack_writable_pages
    }

    /// Return the number of reserved kernel stack virtual pages reclaimed from user tasks.
    pub const fn reclaimed_user_kernel_stack_virtual_pages(self) -> u64 {
        self.reclaimed_user_kernel_stack_virtual_pages
    }
}
