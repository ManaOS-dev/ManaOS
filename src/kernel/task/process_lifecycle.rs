//! User process lifecycle transitions.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

static USER_EXIT_RETURN_STACK: AtomicUsize = AtomicUsize::new(0);
static LAST_USER_EXIT_CODE: AtomicU64 = AtomicU64::new(0);
static LAST_USER_EXIT_TASK_ID: AtomicU64 = AtomicU64::new(0);

/// Result reported by a user task that exited through `SYS_EXIT`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UserTaskExit {
    task_id: u64,
    exit_code: u64,
}

impl UserTaskExit {
    /// Return the task identifier that exited.
    pub fn task_id(&self) -> u64 {
        self.task_id
    }

    /// Return the exit code reported by the task.
    pub fn exit_code(&self) -> u64 {
        self.exit_code
    }
}

/// Run one user-space task until it exits through `SYS_EXIT`.
///
/// Returns the task identifier and exit code reported by the user task that
/// reached `SYS_EXIT`.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_user_task_once(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    task_id: u64,
) -> Option<UserTaskExit> {
    let user_task = {
        let mut scheduler = super::SCHEDULER.lock();
        scheduler
            .as_mut()
            .expect("scheduler must be initialized before running user tasks")
            .prepare_one_shot_user_task(task_id)?
    };
    crate::kernel::memory::address_space::switch_to_user_address_space(user_task.address_space);
    super::install_user_task_kernel_stack(user_task.kernel_stack_top);
    crate::log_info!(
        "task",
        "Installed user task kernel stack: task={} address_space={:#x} top={:#x}",
        task_id,
        user_task.address_space.level_4_frame().as_u64(),
        user_task.kernel_stack_top
    );
    crate::log_info!(
        "task",
        "User trap frame entry prepared: task={} rip={:#x} rsp={:#x} rdi={} rsi={:#x} rdx={:#x}",
        task_id,
        user_task.trap_frame.instruction_pointer,
        user_task.trap_frame.stack_pointer,
        user_task.trap_frame.rdi,
        user_task.trap_frame.rsi,
        user_task.trap_frame.rdx
    );

    LAST_USER_EXIT_TASK_ID.store(0, Ordering::Release);
    super::set_preemption_enabled(true);
    // SAFETY: The trap frame was derived from mapped user code and stack
    // addresses, and this restore path returns only through SYS_EXIT.
    unsafe {
        super::architecture::enter_user_mode_once(user_task.trap_frame.as_pointer());
    }
    super::set_preemption_enabled(false);
    crate::kernel::memory::address_space::switch_to_kernel_address_space();
    crate::log_info!("task", "Restored kernel address space after user exit.");

    let task_id = LAST_USER_EXIT_TASK_ID.load(Ordering::Acquire);
    if task_id == 0 {
        return None;
    }
    let reclaim = {
        let mut scheduler = super::SCHEDULER.lock();
        scheduler
            .as_mut()
            .expect("scheduler must be initialized before reclaiming user address spaces")
            .reclaim_finished_user_address_space(frame_allocator, task_id)
    };
    if let Some(reclaim) = reclaim {
        crate::log_info!(
            "task",
            "User address space reclaimed: task={} user_pages={} page_table_pages={}",
            task_id,
            reclaim.user_pages(),
            reclaim.page_table_pages()
        );
    }

    Some(UserTaskExit {
        task_id,
        exit_code: LAST_USER_EXIT_CODE.load(Ordering::Acquire),
    })
}

/// Mark the currently running user task as finished.
pub fn finish_current_task(exit_code: u64) -> Option<u64> {
    let task_id = super::SCHEDULER
        .lock()
        .as_mut()
        .and_then(super::Scheduler::finish_current_task)?;
    LAST_USER_EXIT_CODE.store(exit_code, Ordering::Release);
    LAST_USER_EXIT_TASK_ID.store(task_id, Ordering::Release);
    Some(task_id)
}

/// Save the kernel return stack used by one-shot user tasks.
#[no_mangle]
pub extern "C" fn set_user_exit_return_stack(stack_pointer: usize) {
    USER_EXIT_RETURN_STACK.store(stack_pointer, Ordering::Release);
}

/// Return the kernel stack pointer restored by `SYS_EXIT`.
#[no_mangle]
pub extern "C" fn get_user_exit_return_stack() -> usize {
    USER_EXIT_RETURN_STACK.load(Ordering::Acquire)
}
