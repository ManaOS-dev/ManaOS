//! Scheduler diagnostics accessors.

use super::{
    current_preemption_state, process_lifecycle, KernelStackGuardFault, Scheduler,
    SchedulerDiagnostics, SchedulerTaskSnapshot, Task, TaskExitStatusDiagnostics, TaskIdentifier,
    TaskKind, TaskRuntimeDiagnosticsSnapshot, TaskState, TaskStateDiagnostics,
    TaskStatusDiagnosticsSnapshot, UserHeapDiagnosticsSnapshot,
    UserMappingActiveDiagnosticsSnapshot, UserMappingLifecycleDiagnosticsSnapshot,
    UserPreemptionReasonDiagnostics, UserResumePathDiagnostics, UserTrapFrameDiagnosticsSnapshot,
    UserVirtualMemorySnapshot, USER_RETURN_PREEMPTION_WINDOW_CLOSE_COUNT,
};
use crate::kernel::task::context::UserTrapFrame;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
impl Scheduler {
    pub(in crate::kernel::task::scheduler) fn get_diagnostics(&self) -> SchedulerDiagnostics {
        let mut ready_tasks = 0_u64;
        let mut running_tasks = 0_u64;
        let mut blocked_tasks = 0_u64;
        let mut finished_tasks = 0_u64;
        let mut kernel_tasks = 0_u64;
        let mut user_tasks = 0_u64;
        let mut active_user_address_spaces = 0_u64;
        let mut retained_user_exit_statuses = 0_u64;
        let mut waitable_user_exit_statuses = 0_u64;
        let mut collected_user_exit_statuses = 0_u64;
        let mut zombie_user_tasks = 0_u64;
        let mut reaped_user_tasks = 0_u64;

        for task in &self.tasks {
            match task.state {
                TaskState::Ready => ready_tasks = ready_tasks.saturating_add(1),
                TaskState::Running => running_tasks = running_tasks.saturating_add(1),
                TaskState::Blocked => blocked_tasks = blocked_tasks.saturating_add(1),
                TaskState::Finished => finished_tasks = finished_tasks.saturating_add(1),
            }
            match &task.kind {
                TaskKind::Kernel => kernel_tasks = kernel_tasks.saturating_add(1),
                TaskKind::User(user_runtime) => {
                    user_tasks = user_tasks.saturating_add(1);
                    if user_runtime.address_space.is_some() {
                        active_user_address_spaces = active_user_address_spaces.saturating_add(1);
                    }
                    if task.metadata.get_exit_code().is_some() {
                        retained_user_exit_statuses = retained_user_exit_statuses.saturating_add(1);
                    }
                    if task.metadata.is_waitable() {
                        waitable_user_exit_statuses = waitable_user_exit_statuses.saturating_add(1);
                        zombie_user_tasks = zombie_user_tasks.saturating_add(1);
                    }
                    if task.metadata.wait_collected() {
                        collected_user_exit_statuses =
                            collected_user_exit_statuses.saturating_add(1);
                        reaped_user_tasks = reaped_user_tasks.saturating_add(1);
                    }
                }
            }
        }

        SchedulerDiagnostics {
            total_tasks: u64::try_from(self.tasks.len()).expect("task count must fit in u64"),
            kernel_tasks,
            user_tasks,
            active_user_tasks: u64::try_from(self.active_user_task_identifiers.len())
                .expect("active user task count must fit in u64"),
            active_user_address_spaces,
            states: TaskStateDiagnostics::new(
                ready_tasks,
                running_tasks,
                blocked_tasks,
                finished_tasks,
            ),
            context_switches: self.context_switch_count,
            timer_preemptions: self.timer_preemption_count,
            user_entries: self.user_entry_count,
            one_shot_user_entries: self.one_shot_user_entry_count,
            timer_user_entries: self.timer_user_entry_count,
            timer_user_entries_from_preempted_user: self.timer_user_entry_from_preempted_user_count,
            user_resumes: self.user_resume_count,
            user_sleep_blocks: self.user_sleep_block_count,
            user_sleep_wakes: self.user_sleep_wake_count,
            user_waitpid_blocks: self.user_waitpid_block_count,
            user_waitpid_wakes: self.user_waitpid_wake_count,
            user_read_blocks: self.user_read_block_count,
            user_read_wakes: self.user_read_wake_count,
            finished_tasks: self.finished_task_count,
            pending_user_exits: u64::try_from(self.finished_user_exits.len())
                .expect("pending user exit count must fit in u64"),
            retained_user_exit_statuses,
            waitable_user_exit_statuses,
            collected_user_exit_statuses,
            zombie_user_tasks,
            reaped_user_tasks,
            preemption_state: current_preemption_state(),
            user_return_preemption_window_closes: USER_RETURN_PREEMPTION_WINDOW_CLOSE_COUNT
                .load(Ordering::Acquire),
            user_return_stack_sets: process_lifecycle::user_return_stack_set_count(),
            user_return_stack_takes: process_lifecycle::user_return_stack_take_count(),
            reclaimed_user_resource_records: self.reclaimed_user_resource_record_count,
            reclaimed_user_address_spaces: self.reclaimed_user_address_space_count,
            reclaimed_user_pages: self.reclaimed_user_page_count,
            reclaimed_user_page_table_pages: self.reclaimed_user_page_table_page_count,
            reclaimed_user_kernel_stacks: self.reclaimed_user_kernel_stack_count,
            reclaimed_user_kernel_stack_writable_pages: self
                .reclaimed_user_kernel_stack_writable_pages,
            reclaimed_user_kernel_stack_virtual_pages: self
                .reclaimed_user_kernel_stack_virtual_pages,
        }
    }

    pub(in crate::kernel::task::scheduler) fn get_task_snapshots(
        &self,
    ) -> Vec<SchedulerTaskSnapshot> {
        self.tasks
            .iter()
            .map(|task| {
                let task_id = task.get_id();
                let parent_task_id = task
                    .metadata
                    .get_parent_identifier()
                    .map(TaskIdentifier::as_u64);
                let active = self.is_user_task_active(task_id);
                match &task.kind {
                    TaskKind::Kernel => SchedulerTaskSnapshot::new_kernel(
                        task_id,
                        parent_task_id,
                        task.state,
                        TaskStatusDiagnosticsSnapshot::new(
                            TaskRuntimeDiagnosticsSnapshot::new(
                                active,
                                false,
                                task.kernel_stack.is_some(),
                                UserTrapFrameDiagnosticsSnapshot::new(0, false, false),
                            ),
                            task_exit_status_diagnostics(task),
                            UserPreemptionReasonDiagnostics::None,
                            UserResumePathDiagnostics::None,
                        ),
                    ),
                    TaskKind::User(user_runtime) => {
                        let user_image = user_runtime.image.snapshot();
                        let user_virtual_memory = UserVirtualMemorySnapshot::new(
                            UserHeapDiagnosticsSnapshot::new(
                                user_runtime.heap.base().as_u64(),
                                user_runtime.heap.current_break().as_u64(),
                                user_runtime.heap.mapped_pages(),
                            ),
                            UserMappingActiveDiagnosticsSnapshot::new(
                                user_runtime.mappings.next_start(),
                                user_runtime.mappings.active_pages(),
                                user_runtime.mappings.active_records(),
                                user_runtime.mappings.active_file_private_records(),
                            ),
                            UserMappingLifecycleDiagnosticsSnapshot::new(
                                user_runtime.mapping_total_mapped_pages,
                                user_runtime.mapping_total_released_pages,
                                user_runtime.mapping_peak_active_pages,
                                user_runtime.mapping_peak_active_records,
                                user_runtime.mapping_file_private_map_count,
                            ),
                        );
                        SchedulerTaskSnapshot::new_user(
                            task_id,
                            parent_task_id,
                            task.state,
                            TaskStatusDiagnosticsSnapshot::new(
                                TaskRuntimeDiagnosticsSnapshot::new(
                                    active,
                                    user_runtime.address_space.is_some(),
                                    task.kernel_stack.is_some(),
                                    UserTrapFrameDiagnosticsSnapshot::new(
                                        core::mem::size_of::<UserTrapFrame>(),
                                        user_runtime.syscall_frame_recorded,
                                        user_runtime.interrupt_frame_recorded,
                                    ),
                                ),
                                task_exit_status_diagnostics(task),
                                user_runtime.last_preemption_reason,
                                user_runtime.last_resume_path,
                            ),
                            &user_image,
                            user_virtual_memory,
                        )
                    }
                }
            })
            .collect()
    }

    pub(in crate::kernel::task::scheduler) fn get_kernel_stack_guard_fault(
        &self,
        fault_address: u64,
    ) -> Option<KernelStackGuardFault> {
        self.tasks
            .iter()
            .find_map(|task| task.kernel_stack_guard_fault(fault_address))
    }

    pub(in crate::kernel::task::scheduler) fn get_kernel_stack_guard_fault_diagnostic_sample(
        &self,
    ) -> Option<KernelStackGuardFault> {
        let sample_guard_address = self
            .tasks
            .iter()
            .find_map(Task::kernel_stack_guard_page_virtual_start)?;
        self.get_kernel_stack_guard_fault(sample_guard_address)
    }
}

fn task_exit_status_diagnostics(task: &Task) -> TaskExitStatusDiagnostics {
    match (
        task.metadata.get_exit_code(),
        task.metadata.wait_collected(),
    ) {
        (Some(exit_code), true) => TaskExitStatusDiagnostics::collected(exit_code),
        (Some(exit_code), false) => TaskExitStatusDiagnostics::waitable(exit_code),
        (None, _) => TaskExitStatusDiagnostics::none(),
    }
}
