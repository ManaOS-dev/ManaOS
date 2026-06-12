//! User process lifecycle transitions.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::architecture;

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
    let user_context = {
        let mut scheduler = super::SCHEDULER.lock();
        scheduler
            .as_mut()
            .expect("scheduler must be initialized before running user tasks")
            .prepare_one_shot_user_task(task_id)?
    };

    // SAFETY: The user task context was created from mapped user code and stack
    // addresses, and this path returns only through SYS_EXIT.
    unsafe {
        architecture::enter_user_mode_once(user_context.as_pointer());
    }

    Some(LAST_USER_EXIT_CODE.load(Ordering::Acquire))
}

/// Mark the currently running user task as finished.
pub fn finish_current_task(exit_code: u64) -> Option<u64> {
    let task_id = super::SCHEDULER
        .lock()
        .as_mut()
        .map(super::Scheduler::finish_current_task)?;
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
