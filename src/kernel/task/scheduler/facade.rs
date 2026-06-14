//! Scheduler public facade and task architecture handoff helpers.

use super::{
    address_space, architecture, current_preemption_state, process_lifecycle,
    KernelStackGuardFault, PhysicalFrameAllocator, PreemptionStateDiagnostics, Scheduler,
    SchedulerDiagnostics, SchedulerTaskSnapshot, SwitchAction, TaskEntry, UserAddressSpace,
    UserAddressSpaceReclaim, UserEntryArguments, UserMappingError, UserMappingRequest,
    UserTaskExit, UserTaskSpawnRequest, UserTrapFrame, UserTrapFrameSource, UserVirtualAddress,
    PREEMPTION_STATE, SCHEDULER, USER_RETURN_PREEMPTION_WINDOW_CLOSE_COUNT,
};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
pub(in crate::kernel::task) fn install_user_task_kernel_stack(kernel_stack_top: usize) {
    let kernel_stack_top =
        u64::try_from(kernel_stack_top).expect("kernel stack top must fit in u64");
    architecture::install_kernel_stack(kernel_stack_top);
    crate::kernel::interrupt::set_syscall_kernel_stack_top(kernel_stack_top);
}

/// Initialize the global scheduler with the current bootstrap task.
pub fn initialize() {
    let mut scheduler = SCHEDULER.lock();
    if scheduler.is_none() {
        *scheduler = Some(Scheduler::new());
    }
}

/// Add a runnable kernel task to the round-robin scheduler.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized, kernel stack frames cannot
/// be allocated, or kernel stack page-table mapping fails.
pub fn spawn(frame_allocator: &mut PhysicalFrameAllocator, entry: TaskEntry) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .expect("scheduler must be initialized before spawning tasks")
        .spawn(frame_allocator, entry)
}

/// Add a runnable user-space task to the round-robin scheduler.
///
/// The spawn origin path is retained for diagnostics and is not changed by a
/// later successful `execve`.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized, kernel stack frames cannot
/// be allocated, or kernel stack page-table mapping fails.
pub fn spawn_user_task(
    frame_allocator: &mut PhysicalFrameAllocator,
    address_space: UserAddressSpace,
    entry_point: UserVirtualAddress,
    user_stack_top: UserVirtualAddress,
    heap_start: UserVirtualAddress,
    entry_arguments: UserEntryArguments,
    spawn_origin_path: &str,
) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .expect("scheduler must be initialized before spawning user tasks")
        .spawn_user_task(
            frame_allocator,
            UserTaskSpawnRequest::new(
                address_space,
                entry_point,
                user_stack_top,
                heap_start,
                entry_arguments,
                spawn_origin_path,
            ),
        )
}

/// Add a user task to the active scheduling set.
pub fn activate_user_task(task_id: u64) -> bool {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .is_some_and(|scheduler| scheduler.activate_user_task(task_id))
}

/// Return whether any active user task records remain schedulable or blocked.
pub fn has_active_user_tasks() -> bool {
    let scheduler = SCHEDULER.lock();
    scheduler
        .as_ref()
        .is_some_and(Scheduler::has_active_user_tasks)
}

/// Run active user-space tasks until one exits through `SYS_EXIT`.
///
/// Starts with `task_id` and returns the exit reported by the active user task
/// that reached `SYS_EXIT`.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_user_task_once(
    frame_allocator: &mut PhysicalFrameAllocator,
    task_id: u64,
) -> Option<UserTaskExit> {
    process_lifecycle::run_user_task_once(frame_allocator, task_id)
}

/// Run the next active user task until one active user task exits.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_next_user_task_once(
    frame_allocator: &mut PhysicalFrameAllocator,
) -> Option<UserTaskExit> {
    let task_id = {
        let scheduler = SCHEDULER.lock();
        scheduler
            .as_ref()
            .expect("scheduler must be initialized before running active user tasks")
            .next_active_user_task_id()?
    };
    run_user_task_once(frame_allocator, task_id)
}

/// Run active user tasks until no runnable active user task remains.
///
/// Returns one exit record for each active user task that exited.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
#[allow(dead_code)]
pub fn run_active_user_tasks_until_empty(
    frame_allocator: &mut PhysicalFrameAllocator,
) -> Vec<UserTaskExit> {
    let mut exits = Vec::new();
    while let Some(exit) = run_next_user_task_once(frame_allocator) {
        exits.push(exit);
    }
    crate::log_info!(
        "task",
        "Active user lifecycle drained: exits={}",
        exits.len()
    );
    exits
}

/// Mark the currently running task as finished.
pub fn finish_current_task(exit_code: u64) -> Option<u64> {
    process_lifecycle::finish_current_task(exit_code)
}

/// Collect one retained child exit status for `parent_task_id`.
pub fn collect_waitable_child_exit(
    parent_task_id: u64,
    child_task_id: Option<u64>,
) -> Option<UserTaskExit> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(|scheduler| scheduler.collect_waitable_child_exit(parent_task_id, child_task_id))
}

/// Return whether the current user task owns a matching child task.
pub fn current_user_task_has_child(child_task_id: Option<u64>) -> Option<bool> {
    let scheduler = SCHEDULER.lock();
    scheduler
        .as_ref()
        .and_then(|scheduler| scheduler.current_user_task_has_child(child_task_id))
}

/// Process a `brk` request for the currently running user task.
pub fn process_current_user_break(
    frame_allocator: &mut PhysicalFrameAllocator,
    requested_break: u64,
) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().and_then(|scheduler| {
        scheduler.process_current_user_break(frame_allocator, requested_break)
    })
}

/// Process a private `mmap` request for the currently running user task.
pub fn process_current_user_mapping(
    frame_allocator: &mut PhysicalFrameAllocator,
    request: UserMappingRequest,
    initialize_page: impl FnMut(u64, &mut [u8]) -> Result<(), UserMappingError>,
) -> Option<Result<u64, UserMappingError>> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().map(|scheduler| {
        scheduler.process_current_user_mapping(frame_allocator, request, initialize_page)
    })
}

/// Process a private `munmap` request for the currently running user task.
pub fn process_current_user_unmapping(
    frame_allocator: &mut PhysicalFrameAllocator,
    start_address: u64,
    length: u64,
) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().and_then(|scheduler| {
        scheduler.process_current_user_unmapping(frame_allocator, start_address, length)
    })
}

/// Prepare the current user task to block until `wake_tick`.
pub fn prepare_current_user_sleep(wake_tick: u64) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(|scheduler| scheduler.prepare_current_user_sleep(wake_tick))
}

/// Block the current user task after its syscall frame has been saved.
pub fn block_current_user_after_syscall() -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(Scheduler::block_current_user_after_syscall)
}

/// Save a captured user trap frame for the currently running user task.
pub fn record_current_user_trap_frame(trap_frame: UserTrapFrame, trap_frame_storage_address: u64) {
    let mut scheduler = SCHEDULER.lock();
    if let Some(scheduler) = scheduler.as_mut() {
        scheduler.record_current_user_trap_frame(
            trap_frame,
            trap_frame_storage_address,
            UserTrapFrameSource::Syscall,
        );
    }
}

/// Save a timer-interrupt user trap frame for the currently running user task.
pub fn record_current_user_interrupt_trap_frame(
    trap_frame: UserTrapFrame,
    trap_frame_storage_address: u64,
) {
    let mut scheduler = SCHEDULER.lock();
    if let Some(scheduler) = scheduler.as_mut() {
        scheduler.record_current_user_trap_frame(
            trap_frame,
            trap_frame_storage_address,
            UserTrapFrameSource::TimerInterrupt,
        );
    }
}

/// Enable or disable timer-driven task switching.
pub fn set_preemption_enabled(enabled: bool) {
    let state = if enabled {
        PreemptionStateDiagnostics::Enabled
    } else {
        PreemptionStateDiagnostics::Disabled
    };
    PREEMPTION_STATE.store(state.as_raw(), Ordering::Release);
}

/// Disable timer-driven task switching while returning to user lifecycle code.
pub fn close_user_return_preemption_window(task_id: u64) {
    PREEMPTION_STATE.store(
        PreemptionStateDiagnostics::UserReturn.as_raw(),
        Ordering::Release,
    );
    USER_RETURN_PREEMPTION_WINDOW_CLOSE_COUNT.fetch_add(1, Ordering::AcqRel);
    crate::log_info!(
        "task",
        "User return preemption window closed: task={}",
        task_id
    );
}

/// Process one timer tick and switch to the next runnable task when possible.
pub fn process_timer_tick(interrupted_user_mode: bool) {
    let switch_action = {
        let Some(mut scheduler) = SCHEDULER.try_lock() else {
            return;
        };

        let Some(scheduler) = scheduler.as_mut() else {
            return;
        };

        scheduler.wake_sleeping_user_tasks(crate::kernel::time::get_timer_ticks());
        if !current_preemption_state().is_enabled() {
            return;
        }

        scheduler.prepare_next_switch(interrupted_user_mode)
    };

    let Some(switch_action) = switch_action else {
        return;
    };

    match switch_action {
        SwitchAction::SwitchKernel {
            current_context,
            next_context,
            next_user_kernel_stack_top,
            next_user_address_space,
        } => {
            if let Some(address_space) = next_user_address_space {
                address_space::switch_to_user_address_space(address_space);
            } else {
                address_space::switch_to_kernel_address_space();
            }
            if let Some(kernel_stack_top) = next_user_kernel_stack_top {
                install_user_task_kernel_stack(kernel_stack_top);
            }
            // SAFETY: Context pointers come from tasks stored in the scheduler.
            // Task stacks are retained by their task objects and switching
            // occurs on one CPU.
            unsafe {
                architecture::switch_context(current_context, next_context);
            }
        }
        SwitchAction::EnterUser {
            current_context,
            task_id,
            trap_frame,
            kernel_stack_top,
            address_space,
        } => {
            address_space::switch_to_user_address_space(address_space);
            install_user_task_kernel_stack(kernel_stack_top);
            crate::log_info!(
                "task",
                "User task entered from timer context: task={} address_space={:#x} kernel_stack_top={:#x}",
                task_id,
                address_space.level_4_frame().as_u64(),
                kernel_stack_top
            );
            // SAFETY: The current context pointer and user trap frame come
            // from tasks stored in the scheduler. The assembly entry saves the
            // current context before consuming the user frame.
            unsafe {
                architecture::switch_to_user_mode(current_context, trap_frame.as_pointer());
            }
        }
    }
}

/// Return the currently selected task identifier.
pub fn get_current_task_id() -> Option<u64> {
    SCHEDULER
        .try_lock()
        .and_then(|scheduler| scheduler.as_ref().map(Scheduler::get_current_task_id))
}

/// Return the currently selected task's parent identifier.
pub fn get_current_parent_task_id() -> Option<u64> {
    SCHEDULER.try_lock().and_then(|scheduler| {
        scheduler
            .as_ref()
            .and_then(Scheduler::get_current_parent_task_id)
    })
}

/// Return the currently selected task's current working directory.
pub fn get_current_working_directory() -> Option<String> {
    SCHEDULER.try_lock().and_then(|scheduler| {
        scheduler
            .as_ref()
            .map(|scheduler| scheduler.get_current_working_directory().into())
    })
}

/// Replace the currently selected task's current working directory.
pub fn set_current_working_directory(path: String) -> Option<()> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().map(|scheduler| {
        scheduler.set_current_working_directory(path);
    })
}

/// Return the currently selected user task address space.
pub fn get_current_user_address_space() -> Option<UserAddressSpace> {
    let scheduler = SCHEDULER.lock();
    scheduler
        .as_ref()
        .and_then(Scheduler::current_user_address_space)
}

/// Replace the currently running user task image and return its old address space.
pub fn replace_current_user_image(
    address_space: UserAddressSpace,
    trap_frame: UserTrapFrame,
    heap_start: UserVirtualAddress,
    image_path: &str,
) -> Option<(u64, UserAddressSpace)> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().and_then(|scheduler| {
        scheduler.replace_current_user_image(address_space, trap_frame, heap_start, image_path)
    })
}

/// Record the old image reclaim result for a successful `execve`.
pub fn record_current_user_execve_reclaim(task_id: u64, reclaim: UserAddressSpaceReclaim) -> bool {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .is_some_and(|scheduler| scheduler.record_current_user_execve_reclaim(task_id, reclaim))
}

/// Return scheduler task counts and lifecycle accounting diagnostics.
pub fn get_scheduler_diagnostics() -> Option<SchedulerDiagnostics> {
    SCHEDULER
        .try_lock()
        .and_then(|scheduler| scheduler.as_ref().map(Scheduler::get_diagnostics))
}

/// Return one snapshot row for each task retained by the scheduler.
pub fn get_scheduler_task_snapshots() -> Option<Vec<SchedulerTaskSnapshot>> {
    SCHEDULER
        .try_lock()
        .and_then(|scheduler| scheduler.as_ref().map(Scheduler::get_task_snapshots))
}

/// Return guard-fault diagnostics when `fault_address` is inside a known kernel
/// stack guard page.
pub fn get_kernel_stack_guard_fault(fault_address: u64) -> Option<KernelStackGuardFault> {
    SCHEDULER.try_lock().and_then(|scheduler| {
        scheduler
            .as_ref()
            .and_then(|scheduler| scheduler.get_kernel_stack_guard_fault(fault_address))
    })
}

/// Return a representative guard-fault diagnostic sample for boot-time checks.
pub fn get_kernel_stack_guard_fault_diagnostic_sample() -> Option<KernelStackGuardFault> {
    SCHEDULER.try_lock().and_then(|scheduler| {
        scheduler
            .as_ref()
            .and_then(Scheduler::get_kernel_stack_guard_fault_diagnostic_sample)
    })
}
