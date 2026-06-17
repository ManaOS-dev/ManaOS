//! Scheduler runtime lifecycle and switching helpers.

use super::{
    address_space, FinishedUserTaskReclaim, KernelStackReclaim, OneShotUserTask,
    PhysicalFrameAllocator, Scheduler, SwitchAction, TaskIdentifier, TaskKind, TaskState,
    UserAddressSpace, UserAddressSpaceReclaim, UserHeap, UserMappings,
    UserPreemptionReasonDiagnostics, UserReadRequest, UserResumePathDiagnostics, UserTaskExit,
    UserTrapFrame, UserTrapFrameSource, UserVirtualAddress, UserWaitpidCompletion,
    UserWaitpidRequest, UserWritableRange, SCHEDULER_TIMER_QUANTUM_TICKS,
    USER_TASK_PREEMPTION_ENABLED,
};
use crate::kernel::memory::address::VirtAddr;
use crate::kernel::memory::user_pointer;

impl Scheduler {
    pub(in crate::kernel::task) fn activate_user_task(&mut self, task_id: u64) -> bool {
        let Some(task_index) = self.get_task_index(task_id) else {
            return false;
        };
        let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
            return false;
        };
        if !user_runtime.has_schedulable_address_space()
            || self.tasks[task_index].state == TaskState::Finished
        {
            return false;
        }
        if !self.is_user_task_active(task_id) {
            self.active_user_task_identifiers.push(task_id);
        }
        self.assert_transition_invariants();
        true
    }

    pub(in crate::kernel::task) fn next_active_user_task_id(&self) -> Option<u64> {
        self.active_user_task_identifiers
            .iter()
            .copied()
            .find(|task_id| {
                let Some(task_index) = self.get_task_index(*task_id) else {
                    return false;
                };
                let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
                    return false;
                };
                self.tasks[task_index].state.is_ready()
                    && user_runtime.has_schedulable_address_space()
            })
    }

    pub(in crate::kernel::task) fn has_active_user_tasks(&self) -> bool {
        !self.active_user_task_identifiers.is_empty()
    }

    pub(in crate::kernel::task) fn deactivate_user_task(&mut self, task_id: u64) {
        self.active_user_task_identifiers
            .retain(|active_task_id| *active_task_id != task_id);
        self.assert_transition_invariants();
    }

    pub(in crate::kernel::task) fn prepare_one_shot_user_task(
        &mut self,
        task_id: u64,
    ) -> Option<OneShotUserTask> {
        let task_index = self.get_task_index(task_id)?;
        let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
            return None;
        };
        if !user_runtime.has_schedulable_address_space() {
            return None;
        }
        let address_space = user_runtime
            .address_space
            .expect("schedulable user tasks must own an address space");
        let trap_frame = user_runtime.saved_frame;
        let kernel_stack_top = self.tasks[task_index]
            .kernel_stack_top()
            .expect("user tasks must own a kernel stack before entry");

        if !self.tasks[task_index].state.is_ready() {
            return None;
        }

        if !self.tasks[self.current_index].state.prepare_to_block() {
            return None;
        }
        if !self.tasks[task_index].state.prepare_to_run() {
            self.tasks[self.current_index].state.resume_blocked();
            return None;
        }
        self.tasks[task_index].context.clear();
        self.set_current_index(task_index);
        self.activate_user_task(task_id);
        self.user_entry_count = self.user_entry_count.saturating_add(1);
        self.one_shot_user_entry_count = self.one_shot_user_entry_count.saturating_add(1);
        if let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind {
            user_runtime.last_resume_path = UserResumePathDiagnostics::LifecycleEntry;
            user_runtime.record_resume_handoff(address_space, kernel_stack_top);
            user_runtime.record_user_trap_frame_restore();
        }
        self.assert_transition_invariants();
        Some(OneShotUserTask {
            trap_frame,
            kernel_stack_top,
            address_space,
        })
    }

    pub(in crate::kernel::task) fn finish_current_task(
        &mut self,
        exit_code: u64,
    ) -> Option<UserTaskExit> {
        let task_id = self.tasks[self.current_index].get_id();
        if !matches!(&self.tasks[self.current_index].kind, TaskKind::User(_)) {
            return None;
        }
        if !self.tasks[self.current_index].state.finish_running() {
            return None;
        }
        assert!(
            self.tasks[self.current_index]
                .metadata
                .record_exit_status(exit_code),
            "user task exit status must be recorded exactly once"
        );
        let parent_task_id = self.tasks[self.current_index]
            .metadata
            .get_parent_identifier()
            .map(TaskIdentifier::as_u64)
            .expect("user tasks must have a parent task identifier");
        self.reparent_orphaned_children_to_initial_process(task_id);

        if let Some(bootstrap_task) = self.tasks.first_mut() {
            if !bootstrap_task.state.resume_blocked() {
                bootstrap_task.state.prepare_to_run();
            }
            self.set_current_index(0);
        }
        self.deactivate_user_task(task_id);
        self.finished_task_count = self.finished_task_count.saturating_add(1);

        let exit = UserTaskExit::new(task_id, exit_code);
        self.finished_user_exits.push_back(exit);
        self.child_exit_records.push(super::ChildExitRecord::new(
            parent_task_id,
            task_id,
            exit_code,
        ));
        crate::log_info!(
            "task",
            "User task exit status retained: parent={} child={} code={} waitable=true",
            parent_task_id,
            task_id,
            exit_code
        );
        crate::log_info!(
            "task",
            "Child exit record retained: parent={} child={} code={} waitable=true",
            parent_task_id,
            task_id,
            exit_code
        );
        self.wake_waiting_parent_for_child_exit(parent_task_id, task_id);
        self.assert_transition_invariants();
        Some(exit)
    }

    fn reparent_orphaned_children_to_initial_process(&mut self, exiting_parent_task_id: u64) {
        let initial_process_task_id = TaskIdentifier::BOOTSTRAP.as_u64();
        let mut reparented_child_count = 0_usize;
        for task in &mut self.tasks {
            if !matches!(&task.kind, TaskKind::User(_)) {
                continue;
            }
            if task
                .metadata
                .get_parent_identifier()
                .map(TaskIdentifier::as_u64)
                != Some(exiting_parent_task_id)
            {
                continue;
            }
            if task.metadata.wait_collected() {
                continue;
            }
            if task.metadata.reparent_to_initial_process() {
                reparented_child_count = reparented_child_count.saturating_add(1);
                crate::log_info!(
                    "task",
                    "Orphaned child reparented: old_parent={} child={} new_parent={}",
                    exiting_parent_task_id,
                    task.get_id(),
                    initial_process_task_id
                );
            }
        }

        let mut reparented_exit_records = 0_usize;
        for record in &mut self.child_exit_records {
            if record.reparent_to_initial_process(exiting_parent_task_id) {
                reparented_exit_records = reparented_exit_records.saturating_add(1);
            }
        }
        if reparented_exit_records > 0 {
            crate::log_info!(
                "task",
                "Orphaned child exit records reparented: old_parent={} new_parent={} records={}",
                exiting_parent_task_id,
                initial_process_task_id,
                reparented_exit_records
            );
        }
        if reparented_child_count > 0 {
            crate::log_info!(
                "task",
                "Orphaned children reparented: old_parent={} new_parent={} children={}",
                exiting_parent_task_id,
                initial_process_task_id,
                reparented_child_count
            );
        }
    }

    pub(in crate::kernel::task) fn take_finished_user_exit(&mut self) -> Option<UserTaskExit> {
        self.finished_user_exits.pop_front()
    }

    pub(in crate::kernel::task) fn collect_waitable_child_exit(
        &mut self,
        parent_task_id: u64,
        child_task_id: Option<u64>,
    ) -> Option<UserTaskExit> {
        let record_index = self
            .child_exit_records
            .iter()
            .position(|record| record.waitable_for_parent(parent_task_id, child_task_id))?;
        let child_task_id = self.child_exit_records[record_index].child_task_id;
        let exit_code = self.child_exit_records[record_index].exit_code;
        let task_index = self
            .get_task_index(child_task_id)
            .expect("child exit record must reference a retained task");
        let collected_exit_code = self.tasks[task_index]
            .metadata
            .collect_waitable_exit()
            .expect("child exit record must reference a waitable task status");
        assert_eq!(
            collected_exit_code, exit_code,
            "child exit record must match retained task exit status"
        );
        self.child_exit_records[record_index].mark_collected();
        crate::log_info!(
            "task",
            "Waitable child exit collected: parent={} child={} code={}",
            parent_task_id,
            child_task_id,
            exit_code
        );

        Some(UserTaskExit::new(child_task_id, exit_code))
    }

    pub(in crate::kernel::task) fn current_user_task_has_child(
        &self,
        child_task_id: Option<u64>,
    ) -> Option<bool> {
        let parent_task_id = self.tasks[self.current_index].get_id();
        if !matches!(&self.tasks[self.current_index].kind, TaskKind::User(_)) {
            return None;
        }

        Some(self.tasks.iter().any(|task| {
            if task
                .metadata
                .get_parent_identifier()
                .map(TaskIdentifier::as_u64)
                != Some(parent_task_id)
            {
                return false;
            }
            if !matches!(&task.kind, TaskKind::User(_)) {
                return false;
            }

            match child_task_id {
                Some(child_task_id) => task.get_id() == child_task_id,
                None => true,
            }
        }))
    }

    pub(in crate::kernel::task) fn prepare_current_user_waitpid(
        &mut self,
        child_task_id: Option<u64>,
        status_buffer: Option<UserWritableRange>,
    ) -> Option<u64> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return None;
        };
        if user_runtime.address_space.is_none()
            || current_task.state != TaskState::Running
            || user_runtime.sleep_wake_tick.is_some()
            || user_runtime.waitpid_request.is_some()
            || user_runtime.waitpid_completion.is_some()
        {
            return None;
        }

        user_runtime.waitpid_request = Some(UserWaitpidRequest::new(child_task_id, status_buffer));
        match child_task_id {
            Some(child_task_id) => crate::log_info!(
                "task",
                "User task waitpid requested: task={} child={}",
                task_id,
                child_task_id
            ),
            None => crate::log_info!(
                "task",
                "User task waitpid requested: task={} child=any",
                task_id
            ),
        }
        Some(task_id)
    }

    pub(in crate::kernel::task) fn prepare_current_user_sleep(
        &mut self,
        wake_tick: u64,
    ) -> Option<u64> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return None;
        };
        if user_runtime.address_space.is_none() || current_task.state != TaskState::Running {
            return None;
        }

        user_runtime.sleep_wake_tick = Some(wake_tick);
        crate::log_info!(
            "task",
            "User task sleep requested: task={} wake_tick={}",
            task_id,
            wake_tick
        );
        Some(task_id)
    }

    pub(in crate::kernel::task) fn prepare_current_user_read(
        &mut self,
        request: UserReadRequest,
    ) -> Option<u64> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return None;
        };
        if user_runtime.address_space.is_none()
            || current_task.state != TaskState::Running
            || user_runtime.sleep_wake_tick.is_some()
            || user_runtime.waitpid_request.is_some()
            || user_runtime.waitpid_completion.is_some()
            || user_runtime.read_request.is_some()
        {
            return None;
        }

        user_runtime.read_request = Some(request);
        crate::log_info!(
            "task",
            "User task read requested: task={} fd={} bytes={}",
            task_id,
            request.file_descriptor(),
            request.byte_len()
        );
        Some(task_id)
    }

    pub(in crate::kernel::task) fn block_current_user_after_syscall(&mut self) -> Option<u64> {
        let task_id = self.tasks[self.current_index].get_id();
        let TaskKind::User(user_runtime) = &self.tasks[self.current_index].kind else {
            return None;
        };
        let wake_tick = user_runtime.sleep_wake_tick;
        let waitpid_request = user_runtime.waitpid_request;
        let read_request = user_runtime.read_request;
        if wake_tick.is_none() && waitpid_request.is_none() && read_request.is_none() {
            return None;
        }
        if !self.tasks[self.current_index].state.prepare_to_block() {
            return None;
        }
        self.tasks[self.current_index].context.clear();
        if let Some(bootstrap_task) = self.tasks.first_mut() {
            if !bootstrap_task.state.resume_blocked() {
                bootstrap_task.state.prepare_to_run();
            }
            self.set_current_index(0);
        }
        if let Some(wake_tick) = wake_tick {
            self.user_sleep_block_count = self.user_sleep_block_count.saturating_add(1);
            crate::log_info!(
                "task",
                "User task blocked for sleep: task={} wake_tick={}",
                task_id,
                wake_tick
            );
        } else if let Some(request) = waitpid_request {
            self.user_waitpid_block_count = self.user_waitpid_block_count.saturating_add(1);
            match request.child_task_id {
                Some(child_task_id) => crate::log_info!(
                    "task",
                    "User task blocked for waitpid: task={} child={}",
                    task_id,
                    child_task_id
                ),
                None => crate::log_info!(
                    "task",
                    "User task blocked for waitpid: task={} child=any",
                    task_id
                ),
            }
        } else if let Some(request) = read_request {
            self.user_read_block_count = self.user_read_block_count.saturating_add(1);
            crate::log_info!(
                "task",
                "User task blocked for read: task={} fd={} bytes={}",
                task_id,
                request.file_descriptor(),
                request.byte_len()
            );
        }
        Some(task_id)
    }

    pub(in crate::kernel::task) fn wake_keyboard_readers(&mut self) -> Option<u64> {
        for task in &mut self.tasks {
            let task_id = task.get_id();
            let TaskKind::User(user_runtime) = &mut task.kind else {
                continue;
            };
            let Some(request) = user_runtime.read_request else {
                continue;
            };
            if !task.state.wake_blocked() {
                continue;
            }
            self.user_read_wake_count = self.user_read_wake_count.saturating_add(1);
            crate::log_info!(
                "task",
                "User task read woke: task={} fd={} bytes={}",
                task_id,
                request.file_descriptor(),
                request.byte_len()
            );
            return Some(task_id);
        }

        None
    }

    pub(in crate::kernel::task) fn is_user_task_blocked_for_read(&self, task_id: u64) -> bool {
        let Some(task_index) = self.get_task_index(task_id) else {
            return false;
        };
        let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
            return false;
        };
        self.tasks[task_index].state == TaskState::Blocked && user_runtime.read_request.is_some()
    }

    pub(in crate::kernel::task) fn take_current_user_read_request(
        &mut self,
        task_id: u64,
    ) -> Option<UserReadRequest> {
        if self.tasks[self.current_index].get_id() != task_id {
            return None;
        }
        let TaskKind::User(user_runtime) = &mut self.tasks[self.current_index].kind else {
            return None;
        };
        user_runtime.read_request.take()
    }

    pub(in crate::kernel::task) fn complete_current_user_read(
        &mut self,
        task_id: u64,
        result: u64,
    ) -> Option<()> {
        if self.tasks[self.current_index].get_id() != task_id {
            return None;
        }
        let TaskKind::User(user_runtime) = &mut self.tasks[self.current_index].kind else {
            return None;
        };
        user_runtime.saved_frame.rax = result;
        crate::log_info!(
            "task",
            "User task read completed: task={} result={}",
            task_id,
            result
        );
        Some(())
    }

    fn wake_waiting_parent_for_child_exit(&mut self, parent_task_id: u64, child_task_id: u64) {
        let Some(parent_index) = self.get_task_index(parent_task_id) else {
            return;
        };
        let TaskKind::User(parent_runtime) = &self.tasks[parent_index].kind else {
            return;
        };
        let Some(request) = parent_runtime.waitpid_request else {
            return;
        };
        if !request.matches_child(child_task_id) {
            return;
        }

        let exit = self
            .collect_waitable_child_exit(parent_task_id, request.child_task_id)
            .expect("waitpid wake must collect the matching child exit record");
        let TaskKind::User(parent_runtime) = &mut self.tasks[parent_index].kind else {
            return;
        };
        parent_runtime.waitpid_request = None;
        parent_runtime.waitpid_completion = Some(UserWaitpidCompletion::new(
            exit.task_id(),
            request.status_buffer,
            exit.wait_status(),
        ));
        parent_runtime.saved_frame.rax = exit.task_id();
        assert!(
            self.tasks[parent_index].state.wake_blocked(),
            "waitpid parent must be blocked before child exit wake"
        );
        self.user_waitpid_wake_count = self.user_waitpid_wake_count.saturating_add(1);
        crate::log_info!(
            "task",
            "User task waitpid woke: parent={} child={} status={}",
            parent_task_id,
            exit.task_id(),
            exit.wait_status()
        );
    }

    pub(in crate::kernel::task) fn complete_pending_user_waitpid_status(
        &mut self,
        task_id: u64,
    ) -> Option<()> {
        let task_index = self.get_task_index(task_id)?;
        let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind else {
            return None;
        };
        let completion = user_runtime.waitpid_completion.take()?;
        if let Some(status_buffer) = completion.status_buffer {
            write_user_wait_status(status_buffer, completion.wait_status);
        }
        crate::log_info!(
            "task",
            "User task waitpid completed: task={} child={} status_stored={}",
            task_id,
            completion.child_task_id,
            completion.status_buffer.is_some()
        );
        Some(())
    }

    pub(in crate::kernel::task) fn wake_sleeping_user_tasks(&mut self, current_tick: u64) {
        for task in &mut self.tasks {
            let task_id = task.get_id();
            let TaskKind::User(user_runtime) = &mut task.kind else {
                continue;
            };
            let Some(wake_tick) = user_runtime.sleep_wake_tick else {
                continue;
            };
            if current_tick < wake_tick {
                continue;
            }
            user_runtime.sleep_wake_tick = None;
            if task.state.wake_blocked() {
                self.user_sleep_wake_count = self.user_sleep_wake_count.saturating_add(1);
                crate::log_info!(
                    "task",
                    "User task sleep woke: task={} wake_tick={} current_tick={}",
                    task_id,
                    wake_tick,
                    current_tick
                );
            }
        }
    }

    pub(in crate::kernel::task) fn reclaim_finished_user_resources(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        task_id: u64,
    ) -> Option<FinishedUserTaskReclaim> {
        let task_index = self.get_task_index(task_id)?;
        if self.tasks[task_index].state != TaskState::Finished {
            return None;
        }
        if !matches!(&self.tasks[task_index].kind, TaskKind::User(_)) {
            return None;
        }

        let address_space_reclaim =
            self.reclaim_finished_user_address_space_at_index(frame_allocator, task_index);
        let kernel_stack_reclaim =
            self.reclaim_finished_user_kernel_stack_at_index(frame_allocator, task_index);
        if let Some(reclaim) = address_space_reclaim {
            self.reclaimed_user_address_space_count =
                self.reclaimed_user_address_space_count.saturating_add(1);
            self.reclaimed_user_page_count = self
                .reclaimed_user_page_count
                .saturating_add(reclaim.user_pages());
            self.reclaimed_user_page_table_page_count = self
                .reclaimed_user_page_table_page_count
                .saturating_add(reclaim.page_table_pages());
        }
        let reclaim = FinishedUserTaskReclaim::new(address_space_reclaim, kernel_stack_reclaim);
        if reclaim.reclaimed_anything() {
            self.reclaimed_user_resource_record_count =
                self.reclaimed_user_resource_record_count.saturating_add(1);
        }
        self.assert_transition_invariants();
        Some(reclaim)
    }

    pub(in crate::kernel::task) fn reclaim_finished_user_address_space_at_index(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        task_index: usize,
    ) -> Option<UserAddressSpaceReclaim> {
        self.begin_finished_user_address_space_reclaim(task_index)?;
        let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind else {
            return None;
        };
        let address_space = user_runtime
            .address_space
            .take()
            .expect("reclaiming user tasks must still own an address space");
        let reclaim = address_space::destroy_user_address_space(frame_allocator, address_space);
        self.finish_finished_user_address_space_reclaim(task_index);
        Some(reclaim)
    }

    fn begin_finished_user_address_space_reclaim(&mut self, task_index: usize) -> Option<()> {
        if self.tasks[task_index].state != TaskState::Finished {
            return None;
        }
        {
            let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind else {
                return None;
            };
            if user_runtime.address_space.is_none() || user_runtime.address_space_reclaiming {
                return None;
            }
            user_runtime.address_space_reclaiming = true;
        }

        assert!(
            !self.user_task_has_schedulable_address_space(task_index),
            "reclaiming user task address space must not be schedulable"
        );
        assert!(
            !self.can_schedule_task(self.current_index, task_index),
            "reclaiming user task must not be a scheduler candidate"
        );
        self.address_space_reclaim_guard_check_count = self
            .address_space_reclaim_guard_check_count
            .saturating_add(1);
        self.assert_transition_invariants();
        Some(())
    }

    fn finish_finished_user_address_space_reclaim(&mut self, task_index: usize) {
        let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind else {
            return;
        };
        user_runtime.address_space_reclaiming = false;
        self.assert_transition_invariants();
    }

    pub(in crate::kernel::task) fn reclaim_finished_user_kernel_stack_at_index(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        task_index: usize,
    ) -> Option<KernelStackReclaim> {
        let kernel_stack = self.tasks[task_index].kernel_stack.take()?;
        let reclaim = kernel_stack.destroy(frame_allocator, &mut self.kernel_stack_range_allocator);
        self.reclaimed_user_kernel_stack_count =
            self.reclaimed_user_kernel_stack_count.saturating_add(1);
        self.reclaimed_user_kernel_stack_writable_pages = self
            .reclaimed_user_kernel_stack_writable_pages
            .saturating_add(reclaim.writable_pages());
        self.reclaimed_user_kernel_stack_virtual_pages = self
            .reclaimed_user_kernel_stack_virtual_pages
            .saturating_add(reclaim.virtual_pages());
        Some(reclaim)
    }

    pub(in crate::kernel::task::scheduler) fn record_current_user_trap_frame(
        &mut self,
        trap_frame: UserTrapFrame,
        trap_frame_storage_address: VirtAddr,
        source: UserTrapFrameSource,
    ) {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let trap_frame_byte_len = u64::try_from(core::mem::size_of::<UserTrapFrame>())
            .expect("user trap frame size must fit in u64");
        let trap_frame_on_kernel_stack = current_task
            .contains_kernel_stack_writable_range(trap_frame_storage_address, trap_frame_byte_len);
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return;
        };

        let should_log = source.should_log(user_runtime);
        user_runtime.saved_frame = trap_frame;
        user_runtime.runtime_trap_frame_record_count = user_runtime
            .runtime_trap_frame_record_count
            .saturating_add(1);
        source.mark_recorded(user_runtime);

        if !should_log {
            return;
        }

        let instruction_pointer = user_runtime
            .saved_frame
            .instruction_pointer_address()
            .expect("saved user trap frame instruction pointer must be a user virtual address");
        let stack_pointer = user_runtime
            .saved_frame
            .stack_pointer_address()
            .expect("saved user trap frame stack pointer must be a user virtual address");

        match source {
            UserTrapFrameSource::Syscall => crate::log_info!(
                "task",
                "User syscall trap frame saved: task={} frame_storage={:#x} on_kernel_stack={} rip={:#x} rsp={:#x} rax={:#x} rdi={:#x} rsi={:#x} rdx={:#x} r10={:#x} r8={:#x} r9={:#x} trap_frame_storage_typed=true trap_frame_user_addresses_typed=true",
                task_id,
                trap_frame_storage_address.as_u64(),
                trap_frame_on_kernel_stack,
                instruction_pointer.as_u64(),
                stack_pointer.as_u64(),
                user_runtime.saved_frame.rax,
                user_runtime.saved_frame.rdi,
                user_runtime.saved_frame.rsi,
                user_runtime.saved_frame.rdx,
                user_runtime.saved_frame.r10,
                user_runtime.saved_frame.r8,
                user_runtime.saved_frame.r9
            ),
            UserTrapFrameSource::TimerInterrupt => crate::log_info!(
                "task",
                "User timer trap frame saved: task={} frame_storage={:#x} on_kernel_stack={} rip={:#x} rsp={:#x} rax={:#x} rcx={:#x} r11={:#x} trap_frame_storage_typed=true trap_frame_user_addresses_typed=true",
                task_id,
                trap_frame_storage_address.as_u64(),
                trap_frame_on_kernel_stack,
                instruction_pointer.as_u64(),
                stack_pointer.as_u64(),
                user_runtime.saved_frame.rax,
                user_runtime.saved_frame.rcx,
                user_runtime.saved_frame.r11
            ),
        }
    }

    pub(in crate::kernel::task) fn can_switch_current_task_away(
        &self,
        interrupted_user_mode: bool,
    ) -> bool {
        match &self.tasks[self.current_index].kind {
            TaskKind::Kernel => true,
            TaskKind::User(user_runtime) => {
                interrupted_user_mode && user_runtime.interrupt_frame_recorded
            }
        }
    }

    fn set_current_index(&mut self, next_index: usize) {
        if self.current_index != next_index {
            self.current_timer_quantum_ticks = 0;
        }
        self.current_index = next_index;
    }

    pub(in crate::kernel::task) fn replace_current_user_image(
        &mut self,
        address_space: UserAddressSpace,
        trap_frame: UserTrapFrame,
        heap_start: UserVirtualAddress,
        image_path: &str,
    ) -> Option<(u64, UserAddressSpace)> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        if current_task.state != TaskState::Running {
            return None;
        }
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return None;
        };
        let old_address_space = user_runtime.address_space.take()?;
        user_runtime.address_space = Some(address_space);

        user_runtime.saved_frame = trap_frame;
        user_runtime.image.replace_with_path(image_path);
        user_runtime.heap = UserHeap::new(heap_start);
        user_runtime.mappings = UserMappings::new();
        user_runtime.mapping_total_mapped_pages = 0;
        user_runtime.mapping_total_released_pages = 0;
        user_runtime.mapping_peak_active_pages = 0;
        user_runtime.mapping_peak_active_records = 0;
        user_runtime.mapping_file_private_map_count = 0;
        user_runtime.sleep_wake_tick = None;
        user_runtime.waitpid_request = None;
        user_runtime.waitpid_completion = None;
        user_runtime.read_request = None;
        user_runtime.syscall_frame_recorded = false;
        user_runtime.interrupt_frame_recorded = false;
        user_runtime.runtime_trap_frame_record_count = 0;
        user_runtime.restored_user_trap_frame_bytes = 0;
        user_runtime.runtime_trap_frame_restore_count = 0;
        user_runtime.resume_handoff_count = 0;
        user_runtime.last_resume_address_space_root = None;
        user_runtime.last_resume_kernel_stack_top = None;
        current_task.context.clear();

        let instruction_pointer = trap_frame
            .instruction_pointer_address()
            .expect("execve trap frame instruction pointer must be a user virtual address");
        let stack_pointer = trap_frame
            .stack_pointer_address()
            .expect("execve trap frame stack pointer must be a user virtual address");
        crate::log_info!(
            "task",
            "User image replaced by execve: task={} old_address_space={:#x} new_address_space={:#x} entry={:#x} stack={:#x} heap_start={:#x} trap_frame_user_addresses_typed=true",
            task_id,
            old_address_space.level_4_frame().as_u64(),
            address_space.level_4_frame().as_u64(),
            instruction_pointer.as_u64(),
            stack_pointer.as_u64(),
            heap_start.as_u64()
        );

        self.assert_transition_invariants();
        Some((task_id, old_address_space))
    }

    pub(in crate::kernel::task) fn verify_transition_invariants(&mut self) -> bool {
        self.assert_transition_invariants();
        self.transition_invariant_check_count =
            self.transition_invariant_check_count.saturating_add(1);
        true
    }

    fn assert_transition_invariants(&self) {
        self.assert_active_user_task_invariants();
        self.assert_user_task_lifecycle_invariants();
    }

    fn assert_active_user_task_invariants(&self) {
        for (active_index, active_task_identifier) in self
            .active_user_task_identifiers
            .iter()
            .copied()
            .enumerate()
        {
            assert!(
                !self.active_user_task_identifiers[..active_index]
                    .contains(&active_task_identifier),
                "active user task set must not contain duplicate task identifiers"
            );
            let task_index = self
                .get_task_index(active_task_identifier)
                .expect("active user task must reference a retained task");
            let task = &self.tasks[task_index];
            let TaskKind::User(user_runtime) = &task.kind else {
                panic!("active user task must reference a user task");
            };
            assert_ne!(
                task.state,
                TaskState::Finished,
                "active user task must not be finished"
            );
            assert!(
                !user_runtime.address_space_reclaiming,
                "active user task must not be reclaiming an address space"
            );
            assert!(
                user_runtime.address_space.is_some(),
                "active user task must own an address space"
            );
            assert!(
                user_runtime.has_schedulable_address_space(),
                "active user task must have a schedulable address space"
            );
        }
    }

    fn assert_user_task_lifecycle_invariants(&self) {
        for task in &self.tasks {
            let TaskKind::User(user_runtime) = &task.kind else {
                continue;
            };
            let task_identifier = task.get_id();

            if task.state == TaskState::Running {
                assert!(
                    self.is_user_task_active(task_identifier),
                    "running user task must remain in the active set"
                );
                assert!(
                    user_runtime.has_schedulable_address_space(),
                    "running user task must have a schedulable address space"
                );
            }
            if task.state == TaskState::Finished {
                assert!(
                    !self.is_user_task_active(task_identifier),
                    "finished user task must not remain in the active set"
                );
            }
            if user_runtime.address_space_reclaiming {
                assert_eq!(
                    task.state,
                    TaskState::Finished,
                    "reclaiming address space must belong to a finished user task"
                );
                assert!(
                    !self.is_user_task_active(task_identifier),
                    "reclaiming user task must not remain in the active set"
                );
                assert!(
                    user_runtime.address_space.is_some(),
                    "reclaiming user task must still own its address space"
                );
                assert!(
                    !user_runtime.has_schedulable_address_space(),
                    "reclaiming user task must not have a schedulable address space"
                );
            }
            if user_runtime.address_space.is_none() {
                assert!(
                    !self.is_user_task_active(task_identifier),
                    "user task without an address space must not remain in the active set"
                );
            }
        }
    }

    pub(in crate::kernel::task) fn record_current_user_execve_reclaim(
        &mut self,
        task_id: u64,
        reclaim: UserAddressSpaceReclaim,
    ) -> bool {
        let Some(task_index) = self.get_task_index(task_id) else {
            return false;
        };
        let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind else {
            return false;
        };
        user_runtime.image.record_last_execve_reclaim(reclaim);
        true
    }

    pub(in crate::kernel::task) fn record_current_user_execve_candidate_drop(&mut self) -> bool {
        let TaskKind::User(user_runtime) = &mut self.tasks[self.current_index].kind else {
            return false;
        };
        user_runtime.image.record_candidate_drop();
        true
    }

    pub(in crate::kernel::task) fn can_schedule_task(
        &self,
        _current_index: usize,
        candidate_index: usize,
    ) -> bool {
        if !self.tasks[candidate_index].state.is_ready() {
            return false;
        }

        match &self.tasks[candidate_index].kind {
            TaskKind::Kernel => !self.tasks[candidate_index].context.is_empty(),
            TaskKind::User(_) => {
                if !USER_TASK_PREEMPTION_ENABLED
                    || !self.is_user_task_active(self.tasks[candidate_index].get_id())
                    || !self.user_task_has_schedulable_address_space(candidate_index)
                {
                    return false;
                }

                true
            }
        }
    }

    pub(in crate::kernel::task) fn user_kernel_stack_top(&self, index: usize) -> Option<VirtAddr> {
        match &self.tasks[index].kind {
            TaskKind::User(_) => Some(
                self.tasks[index]
                    .kernel_stack_top()
                    .expect("user tasks must own a kernel stack before entry or resume"),
            ),
            TaskKind::Kernel => None,
        }
    }

    pub(in crate::kernel::task) fn user_address_space(
        &self,
        index: usize,
    ) -> Option<UserAddressSpace> {
        match &self.tasks[index].kind {
            TaskKind::User(user_runtime) => {
                if user_runtime.address_space_reclaiming {
                    return None;
                }
                user_runtime.address_space
            }
            TaskKind::Kernel => None,
        }
    }

    pub(in crate::kernel::task) fn current_user_address_space(&self) -> Option<UserAddressSpace> {
        self.user_address_space(self.current_index)
    }

    pub(in crate::kernel::task) fn is_first_entry_user_candidate(&self, index: usize) -> bool {
        matches!(self.tasks[index].kind, TaskKind::User(_))
            && self.tasks[index].context.is_empty()
            && self.is_user_task_active(self.tasks[index].get_id())
            && self.user_task_has_schedulable_address_space(index)
    }

    fn user_task_has_schedulable_address_space(&self, index: usize) -> bool {
        match &self.tasks[index].kind {
            TaskKind::User(user_runtime) => user_runtime.has_schedulable_address_space(),
            TaskKind::Kernel => false,
        }
    }

    pub(in crate::kernel::task::scheduler) fn prepare_next_switch(
        &mut self,
        interrupted_user_mode: bool,
    ) -> Option<SwitchAction> {
        if !self.can_switch_current_task_away(interrupted_user_mode) {
            return None;
        }

        self.current_timer_quantum_ticks = self.current_timer_quantum_ticks.saturating_add(1);
        if self.current_timer_quantum_ticks < SCHEDULER_TIMER_QUANTUM_TICKS {
            return None;
        }

        let next_index = self.get_next_ready_index(self.current_index)?;
        if next_index == self.current_index {
            return None;
        }

        let current_index = self.current_index;
        if !self.tasks[current_index].state.prepare_to_wait() {
            return None;
        }
        if !self.tasks[next_index].state.prepare_to_run() {
            return None;
        }
        self.set_current_index(next_index);
        self.context_switch_count = self.context_switch_count.saturating_add(1);

        let current_task_id = self.tasks[current_index].get_id();
        let next_task_id = self.tasks[next_index].get_id();
        let current_task_is_user =
            self.record_current_timer_preemption(current_index, current_task_id, next_task_id);

        let current_context = self.tasks[current_index].context.as_mut_pointer();
        if matches!(self.tasks[next_index].kind, TaskKind::User(_)) {
            return Some(self.prepare_next_user_switch(
                current_context,
                current_task_id,
                next_index,
                current_task_is_user,
            ));
        }

        let next_context = self.tasks[next_index].context.as_pointer();
        Some(SwitchAction::SwitchKernel {
            current_context,
            next_context,
            next_user_kernel_stack_top: None,
            next_user_address_space: None,
        })
    }

    fn record_current_timer_preemption(
        &mut self,
        current_index: usize,
        current_task_id: u64,
        next_task_id: u64,
    ) -> bool {
        let current_task_is_user = matches!(self.tasks[current_index].kind, TaskKind::User(_));
        if !current_task_is_user {
            return false;
        }

        self.timer_preemption_count = self.timer_preemption_count.saturating_add(1);
        if let TaskKind::User(user_runtime) = &mut self.tasks[current_index].kind {
            user_runtime.last_preemption_reason = UserPreemptionReasonDiagnostics::Timer;
        }
        if !self.preemption_switch_logged {
            crate::log_info!(
                "task",
                "User task preempted by timer: current={} next={} context_saved=true",
                current_task_id,
                next_task_id
            );
            self.preemption_switch_logged = true;
        }
        true
    }

    fn prepare_next_user_switch(
        &mut self,
        current_context: *mut u64,
        current_task_id: u64,
        next_index: usize,
        current_task_is_user: bool,
    ) -> SwitchAction {
        let next_task_id = self.tasks[next_index].get_id();
        let kernel_stack_top = self
            .user_kernel_stack_top(next_index)
            .expect("user tasks must own a kernel stack before entry or resume");
        let address_space = self
            .user_address_space(next_index)
            .expect("user tasks must own an address space before entry or resume");

        if self.tasks[next_index].context.is_empty() {
            self.record_timer_user_entry(current_task_id, next_task_id, current_task_is_user);
            return self.prepare_timer_user_entry_switch(
                current_context,
                next_index,
                next_task_id,
                kernel_stack_top,
                address_space,
            );
        }

        self.prepare_timer_user_resume_switch(
            current_context,
            next_index,
            next_task_id,
            kernel_stack_top,
            address_space,
        )
    }

    fn record_timer_user_entry(
        &mut self,
        current_task_id: u64,
        next_task_id: u64,
        current_task_is_user: bool,
    ) {
        self.user_entry_count = self.user_entry_count.saturating_add(1);
        self.timer_user_entry_count = self.timer_user_entry_count.saturating_add(1);
        if current_task_is_user {
            self.timer_user_entry_from_preempted_user_count = self
                .timer_user_entry_from_preempted_user_count
                .saturating_add(1);
            crate::log_info!(
                "task",
                "User task entered from preempted user timer context: current={} next={} first_entry=true",
                current_task_id,
                next_task_id
            );
        }
    }

    fn prepare_timer_user_entry_switch(
        &mut self,
        current_context: *mut u64,
        next_index: usize,
        next_task_id: u64,
        kernel_stack_top: VirtAddr,
        address_space: UserAddressSpace,
    ) -> SwitchAction {
        let TaskKind::User(user_runtime) = &mut self.tasks[next_index].kind else {
            unreachable!("user task kind checked before timer entry");
        };
        user_runtime.last_resume_path = UserResumePathDiagnostics::TimerEntry;
        user_runtime.record_resume_handoff(address_space, kernel_stack_top);
        user_runtime.record_user_trap_frame_restore();
        SwitchAction::EnterUser {
            current_context,
            task_id: next_task_id,
            trap_frame: user_runtime.saved_frame,
            kernel_stack_top,
            address_space,
        }
    }

    fn prepare_timer_user_resume_switch(
        &mut self,
        current_context: *mut u64,
        next_index: usize,
        next_task_id: u64,
        kernel_stack_top: VirtAddr,
        address_space: UserAddressSpace,
    ) -> SwitchAction {
        let next_context = self.tasks[next_index].context.as_pointer();
        let TaskKind::User(user_runtime) = &mut self.tasks[next_index].kind else {
            unreachable!("user task kind checked before timer resume");
        };
        user_runtime.last_resume_path = UserResumePathDiagnostics::TimerResume;
        user_runtime.record_resume_handoff(address_space, kernel_stack_top);
        user_runtime.record_user_trap_frame_restore();
        if !self.user_resume_logged {
            crate::log_info!(
                "task",
                "User task resumed from timer context: task={} kernel_stack_top={:#x} kernel_stack_top_typed=true architecture_stack_installer_typed=true",
                next_task_id,
                kernel_stack_top.as_u64()
            );
            self.user_resume_logged = true;
        }
        self.user_resume_count = self.user_resume_count.saturating_add(1);
        SwitchAction::SwitchKernel {
            current_context,
            next_context,
            next_user_kernel_stack_top: Some(kernel_stack_top),
            next_user_address_space: Some(address_space),
        }
    }

    pub(in crate::kernel::task) fn get_next_ready_index(
        &self,
        current_index: usize,
    ) -> Option<usize> {
        if self.tasks.len() < 2 {
            return None;
        }

        if matches!(self.tasks[current_index].kind, TaskKind::Kernel) {
            for offset in 1..=self.tasks.len() {
                let index = (current_index + offset) % self.tasks.len();
                if self.can_schedule_task(current_index, index)
                    && self.is_first_entry_user_candidate(index)
                {
                    return Some(index);
                }
            }
        }

        for offset in 1..=self.tasks.len() {
            let index = (current_index + offset) % self.tasks.len();
            if self.can_schedule_task(current_index, index) {
                return Some(index);
            }
        }

        None
    }
}

fn write_user_wait_status(status_buffer: UserWritableRange, wait_status: u32) {
    let buffer = user_pointer::copy_to_user(status_buffer)
        .expect("validated blocked waitpid status pointer must remain writable");
    buffer[..core::mem::size_of::<u32>()].copy_from_slice(&wait_status.to_ne_bytes());
}
