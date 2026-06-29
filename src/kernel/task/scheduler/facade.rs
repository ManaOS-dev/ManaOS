//! Scheduler public facade and task architecture handoff helpers.

use super::{
    address_space, architecture, current_preemption_state, process_lifecycle,
    KernelStackGuardFault, PhysicalFrameAllocator, PreemptionStateDiagnostics, Scheduler,
    SchedulerDiagnostics, SchedulerTaskSnapshot, SwitchAction, TaskEntry, UserAddressSpace,
    UserAddressSpaceReclaim, UserEntryArguments, UserMappingError, UserMappingRequest,
    UserMappingUnmapRequest, UserReadRequest, UserTaskExit, UserTaskSpawnRequest, UserTrapFrame,
    UserTrapFrameSource, UserVirtualAddress, PREEMPTION_STATE, SCHEDULER,
    USER_RETURN_PREEMPTION_WINDOW_CLOSE_COUNT,
};
use crate::kernel::filesystem::{FileDescriptorTable, SpawnDescriptorInheritanceSnapshot};
use crate::kernel::memory::address::{UserWritableRange, VirtAddr};
use crate::kernel::memory::user_heap::UserHeapBreakRequest;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
pub(in crate::kernel::task) fn install_user_task_kernel_stack(kernel_stack_top: VirtAddr) {
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

/// Run active user-space tasks until one selected task blocks in `read`.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_user_task_until_read_block(
    frame_allocator: &mut PhysicalFrameAllocator,
    task_id: u64,
) -> Option<()> {
    process_lifecycle::run_user_task_until_read_block(frame_allocator, task_id)
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

/// Prepare the current user task to block in `waitpid`.
pub fn prepare_current_user_waitpid(
    child_task_id: Option<u64>,
    status_buffer: Option<UserWritableRange>,
) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(|scheduler| scheduler.prepare_current_user_waitpid(child_task_id, status_buffer))
}

/// Prepare the current user task to block in `read`.
pub fn prepare_current_user_read(request: UserReadRequest) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(|scheduler| scheduler.prepare_current_user_read(request))
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
    request: UserHeapBreakRequest,
) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(|scheduler| scheduler.process_current_user_break(frame_allocator, request))
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
    request: UserMappingUnmapRequest,
) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(|scheduler| scheduler.process_current_user_unmapping(frame_allocator, request))
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

/// Wake one user task blocked on keyboard-backed `read`.
pub fn wake_keyboard_readers() -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(Scheduler::wake_keyboard_readers)
}

/// Return whether a user task is blocked on `read`.
pub fn is_user_task_blocked_for_read(task_id: u64) -> bool {
    let scheduler = SCHEDULER.lock();
    scheduler
        .as_ref()
        .is_some_and(|scheduler| scheduler.is_user_task_blocked_for_read(task_id))
}

/// Take the current user task's pending `read` request.
pub fn take_current_user_read_request(task_id: u64) -> Option<UserReadRequest> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(|scheduler| scheduler.take_current_user_read_request(task_id))
}

/// Complete the current user task's pending `read` result.
pub fn complete_current_user_read(task_id: u64, result: u64) -> Option<()> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .and_then(|scheduler| scheduler.complete_current_user_read(task_id, result))
}

/// Complete pending `waitpid` user status writes after switching address space.
pub(in crate::kernel::task) fn complete_pending_user_waitpid_status(task_id: u64) {
    let mut scheduler = SCHEDULER.lock();
    if let Some(scheduler) = scheduler.as_mut() {
        let _ = scheduler.complete_pending_user_waitpid_status(task_id);
    }
}

/// Save a captured user trap frame for the currently running user task.
pub fn record_current_user_trap_frame(
    trap_frame: UserTrapFrame,
    trap_frame_storage_address: VirtAddr,
    source: UserTrapFrameSource,
) {
    let mut scheduler = SCHEDULER.lock();
    if let Some(scheduler) = scheduler.as_mut() {
        scheduler.record_current_user_trap_frame(trap_frame, trap_frame_storage_address, source);
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
            mut trap_frame,
            kernel_stack_top,
            address_space,
        } => {
            address_space::switch_to_user_address_space(address_space);
            complete_pending_user_waitpid_status(task_id);
            if let Some(read_result) = crate::kernel::syscall::complete_pending_user_read(task_id) {
                trap_frame.rax = read_result;
            }
            install_user_task_kernel_stack(kernel_stack_top);
            crate::log_info!(
                "task",
                "User task entered from timer context: task={} address_space={:#x} kernel_stack_top={:#x} kernel_stack_top_typed=true architecture_stack_installer_typed=true privilege_stack_top_typed=true",
                task_id,
                address_space.level_4_frame().as_u64(),
                kernel_stack_top.as_u64()
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

/// Replace the currently selected task's file descriptor table.
pub fn replace_current_file_descriptor_table(file_descriptors: FileDescriptorTable) -> Option<()> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().map(|scheduler| {
        scheduler.replace_current_file_descriptor_table(file_descriptors);
    })
}

/// Process the currently selected task's file descriptor table.
pub fn with_current_file_descriptor_table<R>(
    operation: impl FnOnce(&mut FileDescriptorTable) -> R,
) -> Option<R> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .map(|scheduler| scheduler.with_current_file_descriptor_table(operation))
}

/// Clone the currently selected task's file descriptor table.
pub fn clone_current_file_descriptor_table() -> Option<FileDescriptorTable> {
    let scheduler = SCHEDULER.lock();
    scheduler
        .as_ref()
        .map(Scheduler::clone_current_file_descriptor_table)
}

/// Close current task descriptors marked close-on-exec.
pub fn close_current_file_descriptors_on_exec() -> Option<usize> {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .map(Scheduler::close_current_file_descriptors_on_exec)
}

/// Return the current task's spawn descriptor inheritance snapshot.
pub fn get_current_spawn_descriptor_inheritance_snapshot(
) -> Option<SpawnDescriptorInheritanceSnapshot> {
    let scheduler = SCHEDULER.lock();
    scheduler
        .as_ref()
        .map(Scheduler::get_current_spawn_descriptor_inheritance_snapshot)
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

/// Record that the current user task dropped a prepared `execve` candidate.
pub fn record_current_user_execve_candidate_drop() -> bool {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .is_some_and(Scheduler::record_current_user_execve_candidate_drop)
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

/// Verify scheduler transition invariants for active, finished, and reclaiming user tasks.
pub fn verify_scheduler_transition_invariants() -> bool {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .is_some_and(Scheduler::verify_transition_invariants)
}

/// Return guard-fault diagnostics when `fault_address` is inside a known kernel
/// stack guard page.
pub fn get_kernel_stack_guard_fault(fault_address: VirtAddr) -> Option<KernelStackGuardFault> {
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
