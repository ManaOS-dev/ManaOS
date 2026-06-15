//! # `kernel::task`
//!
//! ## Owns
//! - Kernel task metadata API surface
//! - Scheduler module composition and public re-exports
//! - Task context handoff entry points used by interrupt routing
//!
//! ## Does NOT own
//! - Scheduler state transitions (-> `scheduler`)
//! - Architecture-specific register switching (-> `architecture`)
//! - Timer hardware configuration (-> `arch`)
//!
//! ## Public API
//! - [`initialize`] - Initialize the scheduler with the bootstrap task
//! - [`spawn`] - Add a runnable kernel task
//! - [`spawn_user_task`] - Add a runnable user task
//! - [`run_user_task_once`] - Run one user task until `SYS_EXIT`
//! - [`run_user_task_until_read_block`] - Run one user task until it blocks in `read`
//! - [`run_next_user_task_once`] - Run the next active user task until one exits
//! - [`run_active_user_tasks_until_empty`] - Drain active user tasks until none remain
//! - [`UserTaskExit`] - User task exit result
//! - [`UserMappingRequest`] - Syscall-time private mapping request
//! - [`UserReadRequest`] - Syscall-time read wait request
//! - [`process_current_user_break`] - Process a user heap break request
//! - [`process_current_user_mapping`] - Process a private user mapping request
//! - [`process_current_user_unmapping`] - Process a private user unmapping request
//! - [`prepare_current_user_sleep`] - Prepare the current user task to sleep
//! - [`prepare_current_user_waitpid`] - Prepare the current user task to wait for a child
//! - [`prepare_current_user_read`] - Prepare the current user task to wait for input
//! - [`block_current_user_after_syscall`] - Block the current user task after saving its syscall frame
//! - [`wake_keyboard_readers`] - Wake a user task blocked on keyboard input
//! - [`process_timer_tick`] - Run one preemptive scheduling step
//! - [`get_current_task_id`] - Read the current task identifier
//! - [`get_current_parent_task_id`] - Read the current parent task identifier
//! - [`get_current_working_directory`] - Read the current working directory
//! - [`set_current_working_directory`] - Replace the current working directory
//! - [`replace_current_file_descriptor_table`] - Replace the current descriptor table
//! - [`with_current_file_descriptor_table`] - Process the current descriptor table
//! - [`clone_current_file_descriptor_table`] - Clone the current descriptor table
//! - [`close_current_file_descriptors_on_exec`] - Close current close-on-exec descriptors
//! - [`get_current_spawn_descriptor_inheritance_snapshot`] - Read spawn descriptor diagnostics
//! - [`get_current_user_address_space`] - Read the current user task address space
//! - [`collect_waitable_child_exit`] - Collect one retained child exit status
//! - [`current_user_task_has_child`] - Check whether the current user task owns a matching child
//! - [`get_scheduler_diagnostics`] - Read scheduler accounting diagnostics
//! - [`get_scheduler_task_snapshots`] - Read retained task rows for diagnostics
//! - [`verify_scheduler_transition_invariants`] - Check scheduler transition invariants
//! - [`activate_user_task`] - Add a user task to the active scheduling set
//! - [`has_active_user_tasks`] - Check whether active user task records remain
//! - [`set_preemption_enabled`] - Enable or disable timer-driven task switching
//! - [`close_user_return_preemption_window`] - Disable preemption after a user stop syscall
//! - [`record_current_user_trap_frame`] - Save a captured user trap frame
//! - [`UserTrapFrameSource`] - Captured user trap-frame source
//! - [`replace_current_user_image`] - Replace the current user task image during `execve`
//! - [`record_current_user_execve_reclaim`] - Save `execve` old-image reclaim diagnostics
//! - [`record_current_user_execve_candidate_drop`] - Save `execve` candidate-drop diagnostics
//! - [`get_kernel_stack_guard_fault`] - Classify a kernel stack guard fault
//! - [`get_kernel_stack_guard_fault_diagnostic_sample`] - Probe guard-fault diagnostics

pub mod architecture;
pub mod context;
mod diagnostics;
mod metadata;
pub mod process_lifecycle;
mod reclaim;
mod scheduler;
mod stack;
mod state;
pub mod user_mode;

pub use context::UserEntryArguments;
pub use diagnostics::{
    PreemptionStateDiagnostics, SchedulerDiagnostics, SchedulerTaskSnapshot, TaskKindDiagnostics,
    TaskProcessLifecycleDiagnostics, TaskStateDiagnostics, UserExecveReplacementStateDiagnostics,
    UserImageDiagnosticsSnapshot, UserPreemptionReasonDiagnostics, UserResumePathDiagnostics,
    UserVirtualMemorySnapshot,
};
#[allow(unused_imports)]
pub use metadata::{TaskIdentifier, TaskMetadata};
#[allow(unused_imports)]
pub use process_lifecycle::UserTaskExit;
#[allow(unused_imports)]
pub use scheduler::{
    activate_user_task, block_current_user_after_syscall, clone_current_file_descriptor_table,
    close_current_file_descriptors_on_exec, close_user_return_preemption_window,
    collect_waitable_child_exit, complete_current_user_read, current_user_task_has_child,
    finish_current_task, get_current_parent_task_id,
    get_current_spawn_descriptor_inheritance_snapshot, get_current_task_id,
    get_current_user_address_space, get_current_working_directory, get_kernel_stack_guard_fault,
    get_kernel_stack_guard_fault_diagnostic_sample, get_scheduler_diagnostics,
    get_scheduler_task_snapshots, has_active_user_tasks, initialize, is_user_task_blocked_for_read,
    prepare_current_user_read, prepare_current_user_sleep, prepare_current_user_waitpid,
    process_current_user_break, process_current_user_mapping, process_current_user_unmapping,
    process_timer_tick, record_current_user_execve_candidate_drop,
    record_current_user_execve_reclaim, record_current_user_trap_frame,
    replace_current_file_descriptor_table, replace_current_user_image,
    run_active_user_tasks_until_empty, run_next_user_task_once, run_user_task_once,
    run_user_task_until_read_block, set_current_working_directory, set_preemption_enabled, spawn,
    spawn_user_task, take_current_user_read_request, verify_scheduler_transition_invariants,
    wake_keyboard_readers, with_current_file_descriptor_table, Task, UserMappingRequest,
    UserReadRequest, UserTrapFrameSource,
};
#[allow(unused_imports)]
pub use stack::{KernelStackFaultOwner, KernelStackGuardFault};
pub use state::TaskState;
