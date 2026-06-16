//! Scheduler diagnostics snapshots.

use super::state::TaskState;
use crate::kernel::memory::address::{PhysicalFrameStart, VirtAddr};

/// Maximum bytes retained for a task image path diagnostic.
pub(super) const USER_IMAGE_PATH_DIAGNOSTIC_BYTES: usize = 256;

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

/// Process-facing lifecycle state reported by scheduler task snapshots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskProcessLifecycleDiagnostics {
    /// The task is ready to run.
    Ready,
    /// The task is currently running.
    Running,
    /// The task is blocked while waiting for an event.
    Waiting,
    /// The task finished without a retained child-exit status.
    Finished,
    /// The task finished and has a child-exit status waiting for parent reap.
    Zombie,
    /// The task finished and its child-exit status was reaped by its parent.
    Reaped,
}

impl TaskProcessLifecycleDiagnostics {
    /// Return a stable label for console diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Finished => "finished",
            Self::Zombie => "zombie",
            Self::Reaped => "reaped",
        }
    }
}

/// Timer-driven scheduler preemption state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreemptionStateDiagnostics {
    /// Timer-driven task switching is enabled.
    #[default]
    Enabled,
    /// Timer-driven task switching is disabled for a generic kernel reason.
    Disabled,
    /// Timer-driven task switching is disabled while returning to lifecycle code.
    UserReturn,
}

impl PreemptionStateDiagnostics {
    const ENABLED_RAW: u8 = 0;
    const DISABLED_RAW: u8 = 1;
    const USER_RETURN_RAW: u8 = 2;

    /// Return a stable label for console diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::UserReturn => "user_return",
        }
    }

    /// Return whether timer-driven task switching may run.
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }

    pub(super) const fn as_raw(self) -> u8 {
        match self {
            Self::Enabled => Self::ENABLED_RAW,
            Self::Disabled => Self::DISABLED_RAW,
            Self::UserReturn => Self::USER_RETURN_RAW,
        }
    }

    pub(super) const fn from_raw(raw: u8) -> Self {
        if raw == Self::ENABLED_RAW {
            Self::Enabled
        } else if raw == Self::USER_RETURN_RAW {
            Self::UserReturn
        } else {
            Self::Disabled
        }
    }
}

/// Last scheduler preemption reason recorded for one user task.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UserPreemptionReasonDiagnostics {
    /// The task has not been preempted by the scheduler.
    #[default]
    None,
    /// The task was last preempted by a timer interrupt.
    Timer,
}

impl UserPreemptionReasonDiagnostics {
    /// Return a stable label for console diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Timer => "timer",
        }
    }
}

/// Last scheduler path that entered or resumed one user task.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UserResumePathDiagnostics {
    /// The task has not entered user mode.
    #[default]
    None,
    /// The task entered through the lifecycle return path.
    LifecycleEntry,
    /// The task first entered user mode from timer scheduling.
    TimerEntry,
    /// The task resumed a saved timer context.
    TimerResume,
}

impl UserResumePathDiagnostics {
    /// Return a stable label for console diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LifecycleEntry => "lifecycle_entry",
            Self::TimerEntry => "timer_entry",
            Self::TimerResume => "timer_resume",
        }
    }
}

/// Last `execve` replacement lifecycle state reported for one user image.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UserExecveReplacementStateDiagnostics {
    /// No `execve` replacement candidate has been observed for this image.
    #[default]
    None,
    /// The last replacement candidate was dropped before publication.
    CandidateDropped,
    /// The last replacement candidate was published as the active image.
    Published,
}

impl UserExecveReplacementStateDiagnostics {
    /// Return a stable label for console diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CandidateDropped => "candidate_dropped",
            Self::Published => "published",
        }
    }
}

/// Snapshot of the last `execve` replacement diagnostics for one user image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserExecveDiagnosticsSnapshot {
    state: UserExecveReplacementStateDiagnostics,
    old_user_pages: u64,
    old_page_table_pages: u64,
}

impl UserExecveDiagnosticsSnapshot {
    /// Create an `execve` replacement diagnostics snapshot.
    pub(super) const fn new(
        state: UserExecveReplacementStateDiagnostics,
        old_user_pages: u64,
        old_page_table_pages: u64,
    ) -> Self {
        Self {
            state,
            old_user_pages,
            old_page_table_pages,
        }
    }

    /// Return the last observed `execve` replacement lifecycle state.
    pub const fn state(self) -> UserExecveReplacementStateDiagnostics {
        self.state
    }

    /// Return the old user pages reclaimed by the last successful `execve`.
    pub const fn old_user_pages(self) -> u64 {
        self.old_user_pages
    }

    /// Return the old page-table pages reclaimed by the last successful `execve`.
    pub const fn old_page_table_pages(self) -> u64 {
        self.old_page_table_pages
    }
}

/// Snapshot of one user task's current image diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserImageDiagnosticsSnapshot {
    generation: u64,
    origin_path_len: usize,
    origin_path_bytes: [u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES],
    path_len: usize,
    path_bytes: [u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES],
    last_execve: UserExecveDiagnosticsSnapshot,
}

impl UserImageDiagnosticsSnapshot {
    /// Create a user image diagnostics snapshot.
    pub(super) const fn new(
        generation: u64,
        origin_path_len: usize,
        origin_path_bytes: [u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES],
        path_len: usize,
        path_bytes: [u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES],
        last_execve: UserExecveDiagnosticsSnapshot,
    ) -> Self {
        Self {
            generation,
            origin_path_len,
            origin_path_bytes,
            path_len,
            path_bytes,
            last_execve,
        }
    }

    /// Return the current image generation.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Return the number of valid bytes in the retained spawn origin path.
    pub const fn origin_path_len(&self) -> usize {
        self.origin_path_len
    }

    /// Return the retained spawn origin path bytes.
    pub const fn origin_path_bytes(&self) -> &[u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES] {
        &self.origin_path_bytes
    }

    /// Return the number of valid bytes in the retained image path.
    pub const fn path_len(&self) -> usize {
        self.path_len
    }

    /// Return the retained image path bytes.
    pub const fn path_bytes(&self) -> &[u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES] {
        &self.path_bytes
    }

    /// Return the last observed `execve` replacement lifecycle state.
    pub const fn last_execve_state(&self) -> UserExecveReplacementStateDiagnostics {
        self.last_execve.state()
    }

    /// Return the old user pages reclaimed by the last successful `execve`.
    pub const fn last_execve_old_user_pages(&self) -> u64 {
        self.last_execve.old_user_pages()
    }

    /// Return the old page-table pages reclaimed by the last successful `execve`.
    pub const fn last_execve_old_page_table_pages(&self) -> u64 {
        self.last_execve.old_page_table_pages()
    }
}

/// Snapshot of one user task's heap bookkeeping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UserHeapDiagnosticsSnapshot {
    base: u64,
    current_break: u64,
    mapped_pages: u64,
}

impl UserHeapDiagnosticsSnapshot {
    /// Create a user heap diagnostics snapshot.
    pub(super) const fn new(base: u64, current_break: u64, mapped_pages: u64) -> Self {
        Self {
            base,
            current_break,
            mapped_pages,
        }
    }
}

/// Snapshot of one user task's active private mapping state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UserMappingActiveDiagnosticsSnapshot {
    next_start: u64,
    active_pages: u64,
    active_records: u64,
    file_private_records: u64,
}

impl UserMappingActiveDiagnosticsSnapshot {
    /// Create active private mapping diagnostics.
    pub(super) const fn new(
        next_start: u64,
        active_pages: u64,
        active_records: u64,
        file_private_records: u64,
    ) -> Self {
        Self {
            next_start,
            active_pages,
            active_records,
            file_private_records,
        }
    }
}

/// Snapshot of one user task's private mapping lifecycle counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UserMappingLifecycleDiagnosticsSnapshot {
    total_mapped_pages: u64,
    total_released_pages: u64,
    peak_active_pages: u64,
    peak_active_records: u64,
    file_private_map_count: u64,
}

impl UserMappingLifecycleDiagnosticsSnapshot {
    /// Create private mapping lifecycle diagnostics.
    pub(super) const fn new(
        total_mapped_pages: u64,
        total_released_pages: u64,
        peak_active_pages: u64,
        peak_active_records: u64,
        file_private_map_count: u64,
    ) -> Self {
        Self {
            total_mapped_pages,
            total_released_pages,
            peak_active_pages,
            peak_active_records,
            file_private_map_count,
        }
    }
}

/// Snapshot of one user task's virtual memory bookkeeping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserVirtualMemorySnapshot {
    heap: UserHeapDiagnosticsSnapshot,
    mapping_active: UserMappingActiveDiagnosticsSnapshot,
    mapping_lifecycle: UserMappingLifecycleDiagnosticsSnapshot,
}

impl UserVirtualMemorySnapshot {
    /// Create a user virtual memory snapshot from scheduler-owned runtime state.
    pub(super) const fn new(
        heap: UserHeapDiagnosticsSnapshot,
        mapping_active: UserMappingActiveDiagnosticsSnapshot,
        mapping_lifecycle: UserMappingLifecycleDiagnosticsSnapshot,
    ) -> Self {
        Self {
            heap,
            mapping_active,
            mapping_lifecycle,
        }
    }

    /// Return the first virtual address managed by `brk`.
    pub const fn heap_base(self) -> u64 {
        self.heap.base
    }

    /// Return the current user heap break.
    pub const fn heap_break(self) -> u64 {
        self.heap.current_break
    }

    /// Return the number of heap pages currently tracked by the user runtime.
    pub const fn heap_mapped_pages(self) -> u64 {
        self.heap.mapped_pages
    }

    /// Return the next private mapping address candidate.
    pub const fn mapping_next_start(self) -> u64 {
        self.mapping_active.next_start
    }

    /// Return the number of active private mapping pages.
    pub const fn mapping_active_pages(self) -> u64 {
        self.mapping_active.active_pages
    }

    /// Return the number of active private mapping records.
    pub const fn mapping_active_records(self) -> u64 {
        self.mapping_active.active_records
    }

    /// Return the number of active file-private mapping records.
    pub const fn mapping_file_private_records(self) -> u64 {
        self.mapping_active.file_private_records
    }

    /// Return the total pages mapped by successful private mapping syscalls.
    pub const fn mapping_total_mapped_pages(self) -> u64 {
        self.mapping_lifecycle.total_mapped_pages
    }

    /// Return the total private mapping pages released by unmap or replacement.
    pub const fn mapping_total_released_pages(self) -> u64 {
        self.mapping_lifecycle.total_released_pages
    }

    /// Return the highest active private mapping page count observed.
    pub const fn mapping_peak_active_pages(self) -> u64 {
        self.mapping_lifecycle.peak_active_pages
    }

    /// Return the highest active private mapping record count observed.
    pub const fn mapping_peak_active_records(self) -> u64 {
        self.mapping_lifecycle.peak_active_records
    }

    /// Return the number of successful file-private mapping calls.
    pub const fn mapping_file_private_map_count(self) -> u64 {
        self.mapping_lifecycle.file_private_map_count
    }
}

/// Snapshot of one scheduler task record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerTaskSnapshot {
    task_id: u64,
    parent_task_id: Option<u64>,
    kind: TaskKindDiagnostics,
    state: TaskState,
    status: TaskStatusDiagnosticsSnapshot,
    user_image: Option<UserImageDiagnosticsSnapshot>,
    user_virtual_memory: Option<UserVirtualMemorySnapshot>,
}

/// Snapshot of runtime ownership flags for one scheduler task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TaskRuntimeDiagnosticsSnapshot {
    active: bool,
    address_space_owned: bool,
    kernel_stack_owned: bool,
    resume_handoff_count: u64,
    last_resume_address_space_root: Option<PhysicalFrameStart>,
    last_resume_kernel_stack_top: Option<VirtAddr>,
    trap_frame: UserTrapFrameDiagnosticsSnapshot,
}

/// Snapshot of saved user trap frame state for one scheduler task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UserTrapFrameDiagnosticsSnapshot {
    saved_user_trap_frame_bytes: usize,
    syscall_frame_recorded: bool,
    interrupt_frame_recorded: bool,
    runtime_trap_frame_record_count: u64,
    restored_user_trap_frame_bytes: usize,
    runtime_trap_frame_restore_count: u64,
}

impl UserTrapFrameDiagnosticsSnapshot {
    /// Create a user trap frame diagnostics snapshot.
    pub(super) const fn new(
        saved_user_trap_frame_bytes: usize,
        syscall_frame_recorded: bool,
        interrupt_frame_recorded: bool,
        runtime_trap_frame_record_count: u64,
        restored_user_trap_frame_bytes: usize,
        runtime_trap_frame_restore_count: u64,
    ) -> Self {
        Self {
            saved_user_trap_frame_bytes,
            syscall_frame_recorded,
            interrupt_frame_recorded,
            runtime_trap_frame_record_count,
            restored_user_trap_frame_bytes,
            runtime_trap_frame_restore_count,
        }
    }
}

impl TaskRuntimeDiagnosticsSnapshot {
    /// Create a runtime ownership diagnostics snapshot.
    pub(super) const fn new(
        active: bool,
        address_space_owned: bool,
        kernel_stack_owned: bool,
        resume_handoff_count: u64,
        last_resume_address_space_root: Option<PhysicalFrameStart>,
        last_resume_kernel_stack_top: Option<VirtAddr>,
        trap_frame: UserTrapFrameDiagnosticsSnapshot,
    ) -> Self {
        Self {
            active,
            address_space_owned,
            kernel_stack_owned,
            resume_handoff_count,
            last_resume_address_space_root,
            last_resume_kernel_stack_top,
            trap_frame,
        }
    }
}

/// Retained process exit status state for one scheduler task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskExitStatusDiagnostics {
    /// The task has not retained an exit status.
    None,
    /// The task has an exit status that the parent has not collected.
    Waitable(u64),
    /// The task has an exit status that the parent already collected.
    Collected(u64),
}

impl TaskExitStatusDiagnostics {
    /// Create an empty exit-status diagnostic state.
    pub(super) const fn none() -> Self {
        Self::None
    }

    /// Create an uncollected exit-status diagnostic state.
    pub(super) const fn waitable(exit_code: u64) -> Self {
        Self::Waitable(exit_code)
    }

    /// Create a collected exit-status diagnostic state.
    pub(super) const fn collected(exit_code: u64) -> Self {
        Self::Collected(exit_code)
    }

    /// Return the retained exit code when one exists.
    pub const fn exit_code(self) -> Option<u64> {
        match self {
            Self::None => None,
            Self::Waitable(exit_code) | Self::Collected(exit_code) => Some(exit_code),
        }
    }

    /// Return whether this retained exit status has been collected.
    pub const fn wait_collected(self) -> bool {
        matches!(self, Self::Collected(_))
    }
}

/// Snapshot of runtime ownership, exit status, and scheduler path state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TaskStatusDiagnosticsSnapshot {
    runtime: TaskRuntimeDiagnosticsSnapshot,
    exit_status: TaskExitStatusDiagnostics,
    last_preemption_reason: UserPreemptionReasonDiagnostics,
    last_resume_path: UserResumePathDiagnostics,
}

impl TaskStatusDiagnosticsSnapshot {
    /// Create combined task status diagnostics.
    pub(super) const fn new(
        runtime: TaskRuntimeDiagnosticsSnapshot,
        exit_status: TaskExitStatusDiagnostics,
        last_preemption_reason: UserPreemptionReasonDiagnostics,
        last_resume_path: UserResumePathDiagnostics,
    ) -> Self {
        Self {
            runtime,
            exit_status,
            last_preemption_reason,
            last_resume_path,
        }
    }
}

impl SchedulerTaskSnapshot {
    /// Create a kernel task snapshot from scheduler-owned task metadata.
    pub(super) const fn new_kernel(
        task_id: u64,
        parent_task_id: Option<u64>,
        state: TaskState,
        status: TaskStatusDiagnosticsSnapshot,
    ) -> Self {
        Self {
            task_id,
            parent_task_id,
            kind: TaskKindDiagnostics::Kernel,
            state,
            status,
            user_image: None,
            user_virtual_memory: None,
        }
    }

    /// Create a user task snapshot from scheduler-owned task metadata.
    pub(super) const fn new_user(
        task_id: u64,
        parent_task_id: Option<u64>,
        state: TaskState,
        status: TaskStatusDiagnosticsSnapshot,
        user_image: &UserImageDiagnosticsSnapshot,
        user_virtual_memory: UserVirtualMemorySnapshot,
    ) -> Self {
        Self {
            task_id,
            parent_task_id,
            kind: TaskKindDiagnostics::User,
            state,
            status,
            user_image: Some(*user_image),
            user_virtual_memory: Some(user_virtual_memory),
        }
    }

    /// Return the scheduler-local task identifier.
    pub const fn task_id(&self) -> u64 {
        self.task_id
    }

    /// Return the parent task identifier if the task was spawned by another task.
    pub const fn parent_task_id(&self) -> Option<u64> {
        self.parent_task_id
    }

    /// Return the scheduler task kind.
    pub const fn kind(&self) -> TaskKindDiagnostics {
        self.kind
    }

    /// Return the current scheduler lifecycle state.
    pub const fn state(&self) -> TaskState {
        self.state
    }

    /// Return the process-facing lifecycle state for console diagnostics.
    pub const fn process_lifecycle(&self) -> TaskProcessLifecycleDiagnostics {
        match self.status.exit_status {
            TaskExitStatusDiagnostics::Waitable(_) => TaskProcessLifecycleDiagnostics::Zombie,
            TaskExitStatusDiagnostics::Collected(_) => TaskProcessLifecycleDiagnostics::Reaped,
            TaskExitStatusDiagnostics::None => match self.state {
                TaskState::Ready => TaskProcessLifecycleDiagnostics::Ready,
                TaskState::Running => TaskProcessLifecycleDiagnostics::Running,
                TaskState::Blocked => TaskProcessLifecycleDiagnostics::Waiting,
                TaskState::Finished => TaskProcessLifecycleDiagnostics::Finished,
            },
        }
    }

    /// Return whether this user task is in the active scheduling set.
    pub const fn active(&self) -> bool {
        self.status.runtime.active
    }

    /// Return whether this task still owns a user address space.
    pub const fn address_space_owned(&self) -> bool {
        self.status.runtime.address_space_owned
    }

    /// Return whether this task still owns a scheduler-managed kernel stack.
    pub const fn kernel_stack_owned(&self) -> bool {
        self.status.runtime.kernel_stack_owned
    }

    /// Return the number of scheduler handoffs that entered or resumed this user task.
    pub const fn resume_handoff_count(&self) -> u64 {
        self.status.runtime.resume_handoff_count
    }

    /// Return the last user address-space root selected before entry or resume.
    pub const fn last_resume_address_space_root(&self) -> u64 {
        match self.status.runtime.last_resume_address_space_root {
            Some(address_space_root) => address_space_root.as_u64(),
            None => 0,
        }
    }

    /// Return the typed last user address-space root selected before entry or resume.
    pub const fn last_resume_address_space_root_frame(&self) -> Option<PhysicalFrameStart> {
        self.status.runtime.last_resume_address_space_root
    }

    /// Return the last kernel stack top selected before user entry or resume.
    pub const fn last_resume_kernel_stack_top(&self) -> u64 {
        match self.status.runtime.last_resume_kernel_stack_top {
            Some(kernel_stack_top) => kernel_stack_top.as_u64(),
            None => 0,
        }
    }

    /// Return the typed last kernel stack top selected before user entry or resume.
    pub const fn last_resume_kernel_stack_top_address(&self) -> Option<VirtAddr> {
        self.status.runtime.last_resume_kernel_stack_top
    }

    /// Return the byte size of the saved user trap frame, or zero for kernel tasks.
    pub const fn saved_user_trap_frame_bytes(&self) -> usize {
        self.status.runtime.trap_frame.saved_user_trap_frame_bytes
    }

    /// Return whether a returning syscall frame has been recorded for this task.
    pub const fn syscall_frame_recorded(&self) -> bool {
        self.status.runtime.trap_frame.syscall_frame_recorded
    }

    /// Return whether a timer interrupt frame has been recorded for this task.
    pub const fn interrupt_frame_recorded(&self) -> bool {
        self.status.runtime.trap_frame.interrupt_frame_recorded
    }

    /// Return the number of captured runtime user trap frames recorded by the scheduler.
    pub const fn runtime_trap_frame_record_count(&self) -> u64 {
        self.status
            .runtime
            .trap_frame
            .runtime_trap_frame_record_count
    }

    /// Return the byte size of the last restored user trap frame, or zero before entry.
    pub const fn restored_user_trap_frame_bytes(&self) -> usize {
        self.status
            .runtime
            .trap_frame
            .restored_user_trap_frame_bytes
    }

    /// Return the number of restores from a saved runtime user trap frame.
    pub const fn runtime_trap_frame_restore_count(&self) -> u64 {
        self.status
            .runtime
            .trap_frame
            .runtime_trap_frame_restore_count
    }

    /// Return the retained exit code for finished task records.
    pub const fn exit_code(&self) -> Option<u64> {
        self.status.exit_status.exit_code()
    }

    /// Return whether the retained exit code has been collected by the parent.
    pub const fn wait_collected(&self) -> bool {
        self.status.exit_status.wait_collected()
    }

    /// Return the last scheduler preemption reason recorded for this task.
    pub const fn last_preemption_reason(&self) -> UserPreemptionReasonDiagnostics {
        self.status.last_preemption_reason
    }

    /// Return the last scheduler path that entered or resumed this task.
    pub const fn last_resume_path(&self) -> UserResumePathDiagnostics {
        self.status.last_resume_path
    }

    /// Return user image diagnostics for user task records.
    pub fn user_image(&self) -> Option<&UserImageDiagnosticsSnapshot> {
        self.user_image.as_ref()
    }

    /// Return user virtual memory bookkeeping for user task records.
    pub const fn user_virtual_memory(&self) -> Option<UserVirtualMemorySnapshot> {
        self.user_virtual_memory
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
    pub(super) one_shot_user_entries: u64,
    pub(super) timer_user_entries: u64,
    pub(super) timer_user_entries_from_preempted_user: u64,
    pub(super) user_resumes: u64,
    pub(super) user_sleep_blocks: u64,
    pub(super) user_sleep_wakes: u64,
    pub(super) user_waitpid_blocks: u64,
    pub(super) user_waitpid_wakes: u64,
    pub(super) user_read_blocks: u64,
    pub(super) user_read_wakes: u64,
    pub(super) finished_tasks: u64,
    pub(super) pending_user_exits: u64,
    pub(super) retained_user_exit_statuses: u64,
    pub(super) waitable_user_exit_statuses: u64,
    pub(super) collected_user_exit_statuses: u64,
    pub(super) zombie_user_tasks: u64,
    pub(super) reaped_user_tasks: u64,
    pub(super) preemption_state: PreemptionStateDiagnostics,
    pub(super) user_return_preemption_window_closes: u64,
    pub(super) user_return_stack_sets: u64,
    pub(super) user_return_stack_takes: u64,
    pub(super) reclaimed_user_resource_records: u64,
    pub(super) reclaimed_user_address_spaces: u64,
    pub(super) reclaimed_user_pages: u64,
    pub(super) reclaimed_user_page_table_pages: u64,
    pub(super) reclaimed_user_kernel_stacks: u64,
    pub(super) reclaimed_user_kernel_stack_writable_pages: u64,
    pub(super) reclaimed_user_kernel_stack_virtual_pages: u64,
    pub(super) address_space_reclaim_guard_checks: u64,
    pub(super) scheduler_transition_invariant_checks: u64,
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

    /// Return the number of returnable lifecycle entries into a user task.
    pub const fn one_shot_user_entries(self) -> u64 {
        self.one_shot_user_entries
    }

    /// Return the number of first user entries started by timer scheduling.
    pub const fn timer_user_entries(self) -> u64 {
        self.timer_user_entries
    }

    /// Return the number of timer-started user entries from a preempted user task.
    pub const fn timer_user_entries_from_preempted_user(self) -> u64 {
        self.timer_user_entries_from_preempted_user
    }

    /// Return the number of resumes into an already-started user task.
    pub const fn user_resumes(self) -> u64 {
        self.user_resumes
    }

    /// Return the number of user tasks blocked by sleep syscalls.
    pub const fn user_sleep_blocks(self) -> u64 {
        self.user_sleep_blocks
    }

    /// Return the number of user tasks woken after sleep deadlines.
    pub const fn user_sleep_wakes(self) -> u64 {
        self.user_sleep_wakes
    }

    /// Return the number of user tasks blocked by waitpid syscalls.
    pub const fn user_waitpid_blocks(self) -> u64 {
        self.user_waitpid_blocks
    }

    /// Return the number of user tasks woken after matching child exits.
    pub const fn user_waitpid_wakes(self) -> u64 {
        self.user_waitpid_wakes
    }

    /// Return the number of user tasks blocked by read syscalls.
    pub const fn user_read_blocks(self) -> u64 {
        self.user_read_blocks
    }

    /// Return the number of user tasks woken after readable input arrived.
    pub const fn user_read_wakes(self) -> u64 {
        self.user_read_wakes
    }

    /// Return the number of tasks marked finished through the scheduler.
    pub const fn finished_tasks(self) -> u64 {
        self.finished_tasks
    }

    /// Return the number of finished user exits waiting to be reported.
    pub const fn pending_user_exits(self) -> u64 {
        self.pending_user_exits
    }

    /// Return the number of retained finished user exit statuses.
    pub const fn retained_user_exit_statuses(self) -> u64 {
        self.retained_user_exit_statuses
    }

    /// Return the number of retained user exits still available to a parent wait.
    pub const fn waitable_user_exit_statuses(self) -> u64 {
        self.waitable_user_exit_statuses
    }

    /// Return the number of retained user exits already collected by a parent wait.
    pub const fn collected_user_exit_statuses(self) -> u64 {
        self.collected_user_exit_statuses
    }

    /// Return the number of finished user child tasks waiting for parent reap.
    pub const fn zombie_user_tasks(self) -> u64 {
        self.zombie_user_tasks
    }

    /// Return the number of user child tasks already reaped by their parent.
    pub const fn reaped_user_tasks(self) -> u64 {
        self.reaped_user_tasks
    }

    /// Return whether timer-driven task switching is currently enabled.
    pub const fn preemption_enabled(self) -> bool {
        self.preemption_state.is_enabled()
    }

    /// Return the current timer-driven scheduler preemption state.
    pub const fn preemption_state(self) -> PreemptionStateDiagnostics {
        self.preemption_state
    }

    /// Return the number of user stop syscalls that closed the preemption window.
    pub const fn user_return_preemption_window_closes(self) -> u64 {
        self.user_return_preemption_window_closes
    }

    /// Return the number of returnable user stacks stored before Ring 3 entry.
    pub const fn user_return_stack_sets(self) -> u64 {
        self.user_return_stack_sets
    }

    /// Return the number of returnable user stacks consumed by user stop syscalls.
    pub const fn user_return_stack_takes(self) -> u64 {
        self.user_return_stack_takes
    }

    /// Return the number of finished user task resource reclaim records.
    pub const fn reclaimed_user_resource_records(self) -> u64 {
        self.reclaimed_user_resource_records
    }

    /// Return the number of finished user address spaces reclaimed.
    pub const fn reclaimed_user_address_spaces(self) -> u64 {
        self.reclaimed_user_address_spaces
    }

    /// Return the number of user data pages reclaimed from finished address spaces.
    pub const fn reclaimed_user_pages(self) -> u64 {
        self.reclaimed_user_pages
    }

    /// Return the number of page-table pages reclaimed from finished address spaces.
    pub const fn reclaimed_user_page_table_pages(self) -> u64 {
        self.reclaimed_user_page_table_pages
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

    /// Return the number of reclaim-time scheduling guard checks.
    pub const fn address_space_reclaim_guard_checks(self) -> u64 {
        self.address_space_reclaim_guard_checks
    }

    /// Return the number of explicit scheduler transition invariant checks.
    pub const fn scheduler_transition_invariant_checks(self) -> u64 {
        self.scheduler_transition_invariant_checks
    }
}
