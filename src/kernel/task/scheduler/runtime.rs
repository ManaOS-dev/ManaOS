//! Scheduler runtime lifecycle and switching helpers.

use super::{
    address_space, FinishedUserTaskReclaim, KernelStackReclaim, OneShotUserTask,
    PhysicalFrameAllocator, Scheduler, SwitchAction, TaskIdentifier, TaskKind, TaskState,
    UserAddressSpace, UserAddressSpaceReclaim, UserHeap, UserMappings,
    UserPreemptionReasonDiagnostics, UserReadRequest, UserResumePathDiagnostics, UserTaskExit,
    UserTrapFrame, UserTrapFrameSource, UserVirtualAddress, UserVirtualRange,
    UserWaitpidCompletion, UserWaitpidRequest, UserWritableRange, USER_TASK_PREEMPTION_ENABLED,
};
use crate::kernel::memory::user_pointer;

const USER_WAIT_STATUS_BYTES: u64 = core::mem::size_of::<i32>() as u64;
impl Scheduler {
    pub(in crate::kernel::task) fn activate_user_task(&mut self, task_id: u64) -> bool {
        let Some(task_index) = self.get_task_index(task_id) else {
            return false;
        };
        let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
            return false;
        };
        if user_runtime.address_space.is_none()
            || self.tasks[task_index].state == TaskState::Finished
        {
            return false;
        }
        if !self.is_user_task_active(task_id) {
            self.active_user_task_identifiers.push(task_id);
        }
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
                self.tasks[task_index].state.is_ready() && user_runtime.address_space.is_some()
            })
    }

    pub(in crate::kernel::task) fn has_active_user_tasks(&self) -> bool {
        !self.active_user_task_identifiers.is_empty()
    }

    pub(in crate::kernel::task) fn deactivate_user_task(&mut self, task_id: u64) {
        self.active_user_task_identifiers
            .retain(|active_task_id| *active_task_id != task_id);
    }

    pub(in crate::kernel::task) fn prepare_one_shot_user_task(
        &mut self,
        task_id: u64,
    ) -> Option<OneShotUserTask> {
        let task_index = self.get_task_index(task_id)?;
        let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
            return None;
        };
        let address_space = user_runtime.address_space?;
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
        self.current_index = task_index;
        self.activate_user_task(task_id);
        self.user_entry_count = self.user_entry_count.saturating_add(1);
        self.one_shot_user_entry_count = self.one_shot_user_entry_count.saturating_add(1);
        if let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind {
            user_runtime.last_resume_path = UserResumePathDiagnostics::LifecycleEntry;
            user_runtime.record_user_trap_frame_restore();
        }
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
            self.current_index = 0;
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
        status_pointer: Option<u64>,
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

        user_runtime.waitpid_request = Some(UserWaitpidRequest::new(child_task_id, status_pointer));
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
            self.current_index = 0;
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
            request.status_pointer,
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
        if let Some(status_pointer) = completion.status_pointer {
            write_user_wait_status(status_pointer, completion.wait_status);
        }
        crate::log_info!(
            "task",
            "User task waitpid completed: task={} child={} status_stored={}",
            task_id,
            completion.child_task_id,
            completion.status_pointer.is_some()
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
        Some(reclaim)
    }

    pub(in crate::kernel::task) fn reclaim_finished_user_address_space_at_index(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        task_index: usize,
    ) -> Option<UserAddressSpaceReclaim> {
        let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind else {
            return None;
        };
        let address_space = user_runtime.address_space.take()?;
        Some(address_space::destroy_user_address_space(
            frame_allocator,
            address_space,
        ))
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
        trap_frame_storage_address: u64,
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

        match source {
            UserTrapFrameSource::Syscall => crate::log_info!(
                "task",
                "User syscall trap frame saved: task={} frame_storage={:#x} on_kernel_stack={} rip={:#x} rsp={:#x} rax={:#x} rdi={:#x} rsi={:#x} rdx={:#x} r10={:#x} r8={:#x} r9={:#x}",
                task_id,
                trap_frame_storage_address,
                trap_frame_on_kernel_stack,
                user_runtime.saved_frame.instruction_pointer,
                user_runtime.saved_frame.stack_pointer,
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
                "User timer trap frame saved: task={} frame_storage={:#x} on_kernel_stack={} rip={:#x} rsp={:#x} rax={:#x} rcx={:#x} r11={:#x}",
                task_id,
                trap_frame_storage_address,
                trap_frame_on_kernel_stack,
                user_runtime.saved_frame.instruction_pointer,
                user_runtime.saved_frame.stack_pointer,
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
        current_task.context.clear();

        crate::log_info!(
            "task",
            "User image replaced by execve: task={} old_address_space={:#x} new_address_space={:#x} entry={:#x} stack={:#x} heap_start={:#x}",
            task_id,
            old_address_space.level_4_frame().as_u64(),
            address_space.level_4_frame().as_u64(),
            trap_frame.instruction_pointer,
            trap_frame.stack_pointer,
            heap_start.as_u64()
        );

        Some((task_id, old_address_space))
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
                {
                    return false;
                }

                true
            }
        }
    }

    pub(in crate::kernel::task) fn user_kernel_stack_top(&self, index: usize) -> Option<usize> {
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
            TaskKind::User(user_runtime) => user_runtime.address_space,
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
    }

    pub(in crate::kernel::task::scheduler) fn prepare_next_switch(
        &mut self,
        interrupted_user_mode: bool,
    ) -> Option<SwitchAction> {
        if !self.can_switch_current_task_away(interrupted_user_mode) {
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
        self.current_index = next_index;
        self.context_switch_count = self.context_switch_count.saturating_add(1);

        let current_task_id = self.tasks[current_index].get_id();
        let next_task_id = self.tasks[next_index].get_id();
        let current_task_is_user = matches!(self.tasks[current_index].kind, TaskKind::User(_));
        if current_task_is_user {
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
        }

        let current_context = self.tasks[current_index].context.as_mut_pointer();
        let next_user_kernel_stack_top = self.user_kernel_stack_top(next_index);
        let next_user_address_space = self.user_address_space(next_index);
        let next_context_is_empty = self.tasks[next_index].context.is_empty();
        if matches!(self.tasks[next_index].kind, TaskKind::User(_)) {
            if next_context_is_empty {
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
                let TaskKind::User(user_runtime) = &mut self.tasks[next_index].kind else {
                    unreachable!("user task kind checked before timer entry");
                };
                user_runtime.last_resume_path = UserResumePathDiagnostics::TimerEntry;
                user_runtime.record_user_trap_frame_restore();
                return Some(SwitchAction::EnterUser {
                    current_context,
                    task_id: next_task_id,
                    trap_frame: user_runtime.saved_frame,
                    kernel_stack_top: next_user_kernel_stack_top
                        .expect("user tasks must own a kernel stack before entry"),
                    address_space: next_user_address_space
                        .expect("user tasks must own an address space before entry"),
                });
            }
            let TaskKind::User(user_runtime) = &mut self.tasks[next_index].kind else {
                unreachable!("user task kind checked before timer resume");
            };
            user_runtime.last_resume_path = UserResumePathDiagnostics::TimerResume;
            user_runtime.record_user_trap_frame_restore();
            if !self.user_resume_logged {
                crate::log_info!(
                    "task",
                    "User task resumed from timer context: task={} kernel_stack_top={:#x}",
                    next_task_id,
                    next_user_kernel_stack_top
                        .expect("user tasks must own a kernel stack before resume")
                );
                self.user_resume_logged = true;
            }
            self.user_resume_count = self.user_resume_count.saturating_add(1);
        }

        let next_context = self.tasks[next_index].context.as_pointer();
        Some(SwitchAction::SwitchKernel {
            current_context,
            next_context,
            next_user_kernel_stack_top,
            next_user_address_space,
        })
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

fn write_user_wait_status(status_pointer: u64, wait_status: u32) {
    let range = UserVirtualRange::from_syscall_arguments(status_pointer, USER_WAIT_STATUS_BYTES)
        .expect("validated waitpid status pointer must remain in user range");
    let buffer = user_pointer::copy_to_user(UserWritableRange::new(range))
        .expect("validated blocked waitpid status pointer must remain writable");
    buffer[..core::mem::size_of::<u32>()].copy_from_slice(&wait_status.to_ne_bytes());
}
