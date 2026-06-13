//! User process lifecycle transitions.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

static USER_EXIT_RETURN_STACK: AtomicUsize = AtomicUsize::new(0);
static LAST_USER_EXIT_CODE: AtomicU64 = AtomicU64::new(0);

/// Run one user-space task until it exits through `SYS_EXIT`.
///
/// Returns the exit code reported by the user task.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_user_task_once(task_id: u64) -> Option<u64> {
    let user_task = {
        let mut scheduler = super::SCHEDULER.lock();
        scheduler
            .as_mut()
            .expect("scheduler must be initialized before running user tasks")
            .prepare_one_shot_user_task(task_id)?
    };
    super::install_user_task_kernel_stack(user_task.kernel_stack_top);
    crate::log_info!(
        "task",
        "Installed user task kernel stack: task={} top={:#x}",
        task_id,
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

    // SAFETY: The trap frame was derived from mapped user code and stack
    // addresses, and this restore path returns only through SYS_EXIT.
    unsafe {
        super::architecture::enter_user_mode_once(user_task.trap_frame.as_pointer());
    }

    Some(LAST_USER_EXIT_CODE.load(Ordering::Acquire))
}

/// Mark the currently running user task as finished.
pub fn finish_current_task(exit_code: u64) -> Option<u64> {
    let task_id = super::SCHEDULER
        .lock()
        .as_mut()
        .and_then(super::Scheduler::finish_current_task)?;
    LAST_USER_EXIT_CODE.store(exit_code, Ordering::Release);
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
