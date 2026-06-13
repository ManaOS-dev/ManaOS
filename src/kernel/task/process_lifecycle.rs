//! User process lifecycle transitions.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

static USER_EXIT_RETURN_STACK: AtomicUsize = AtomicUsize::new(0);
static USER_EXIT_RETURN_WINDOW_TASK_ID: AtomicU64 = AtomicU64::new(0);
static USER_EXIT_RETURN_STACK_SET_COUNT: AtomicU64 = AtomicU64::new(0);
static USER_EXIT_RETURN_STACK_TAKE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Result reported by a user task that exited through `SYS_EXIT`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UserTaskExit {
    task_id: u64,
    exit_code: u64,
}

impl UserTaskExit {
    /// Create an exit result for a finished user task.
    pub(super) const fn new(task_id: u64, exit_code: u64) -> Self {
        Self { task_id, exit_code }
    }

    /// Return the task identifier that exited.
    pub fn task_id(&self) -> u64 {
        self.task_id
    }

    /// Return the exit code reported by the task.
    pub fn exit_code(&self) -> u64 {
        self.exit_code
    }
}

/// Run active user-space tasks until one exits through `SYS_EXIT`.
///
/// Starts with `task_id` and returns the task identifier and exit code reported
/// by the active user task that reached `SYS_EXIT`.
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

    begin_user_exit_return_window(task_id);
    crate::kernel::memory::runtime_allocator::register_user_runtime_frame_allocator(
        frame_allocator,
    );
    super::set_preemption_enabled(true);
    // SAFETY: The trap frame was derived from mapped user code and stack
    // addresses, and this restore path returns only through SYS_EXIT.
    unsafe {
        super::architecture::enter_user_mode_once(user_task.trap_frame.as_pointer());
    }
    super::set_preemption_enabled(false);
    crate::kernel::memory::runtime_allocator::clear_user_runtime_frame_allocator();
    crate::kernel::memory::address_space::switch_to_kernel_address_space();
    assert_user_exit_return_window_consumed();
    crate::log_info!("task", "Restored kernel address space after user exit.");

    let (exit, resource_reclaim) = {
        let mut scheduler = super::SCHEDULER.lock();
        let scheduler = scheduler
            .as_mut()
            .expect("scheduler must be initialized before reclaiming finished user task resources");
        let exit = scheduler.take_finished_user_exit()?;
        let task_id = exit.task_id();
        let resource_reclaim = scheduler
            .reclaim_finished_user_resources(frame_allocator, task_id)
            .expect("finished user exit must reference a reclaimable user task");
        (exit, resource_reclaim)
    };
    let task_id = exit.task_id();
    if let Some(reclaim) = resource_reclaim.address_space() {
        crate::log_info!(
            "task",
            "User address space reclaimed: task={} user_pages={} page_table_pages={}",
            task_id,
            reclaim.user_pages(),
            reclaim.page_table_pages()
        );
    }
    if let Some(reclaim) = resource_reclaim.kernel_stack() {
        crate::log_info!(
            "task",
            "User kernel stack reclaimed: task={} writable_pages={} virtual_pages={}",
            task_id,
            reclaim.writable_pages(),
            reclaim.virtual_pages()
        );
    }
    crate::log_info!(
        "task",
        "User task resources reclaimed: task={} address_space={} kernel_stack={}",
        task_id,
        resource_reclaim.reclaimed_address_space(),
        resource_reclaim.reclaimed_kernel_stack()
    );

    Some(exit)
}

/// Mark the currently running user task as finished.
pub fn finish_current_task(exit_code: u64) -> Option<u64> {
    let exit = super::SCHEDULER
        .lock()
        .as_mut()
        .and_then(|scheduler| scheduler.finish_current_task(exit_code))?;
    Some(exit.task_id())
}

fn begin_user_exit_return_window(task_id: u64) {
    assert_ne!(
        task_id, 0,
        "user exit return window task id must be non-zero"
    );
    assert_eq!(
        USER_EXIT_RETURN_STACK.load(Ordering::Acquire),
        0,
        "user exit return stack must be clear before entering user mode"
    );
    USER_EXIT_RETURN_WINDOW_TASK_ID
        .compare_exchange(0, task_id, Ordering::AcqRel, Ordering::Acquire)
        .expect("user exit return window must not already be active");
}

fn assert_user_exit_return_window_consumed() {
    assert_eq!(
        USER_EXIT_RETURN_STACK.load(Ordering::Acquire),
        0,
        "user exit return stack must be consumed before returning to lifecycle code"
    );
    assert_eq!(
        USER_EXIT_RETURN_WINDOW_TASK_ID.load(Ordering::Acquire),
        0,
        "user exit return window task id must be consumed before lifecycle cleanup"
    );
}

/// Save the kernel return stack used by one-shot user tasks.
#[no_mangle]
pub extern "C" fn set_user_exit_return_stack(stack_pointer: usize) {
    assert_ne!(
        stack_pointer, 0,
        "user exit return stack pointer must be non-zero"
    );
    assert_ne!(
        USER_EXIT_RETURN_WINDOW_TASK_ID.load(Ordering::Acquire),
        0,
        "user exit return window must be active before storing its stack"
    );
    assert_eq!(
        USER_EXIT_RETURN_STACK.swap(stack_pointer, Ordering::AcqRel),
        0,
        "user exit return stack must be stored exactly once per entry"
    );
    USER_EXIT_RETURN_STACK_SET_COUNT.fetch_add(1, Ordering::AcqRel);
}

/// Return the kernel stack pointer restored by `SYS_EXIT`.
#[no_mangle]
pub extern "C" fn get_user_exit_return_stack() -> usize {
    let stack_pointer = USER_EXIT_RETURN_STACK.swap(0, Ordering::AcqRel);
    assert_ne!(
        stack_pointer, 0,
        "user exit return stack must be available before SYS_EXIT return"
    );
    assert_ne!(
        USER_EXIT_RETURN_WINDOW_TASK_ID.swap(0, Ordering::AcqRel),
        0,
        "user exit return window must be active before SYS_EXIT return"
    );
    USER_EXIT_RETURN_STACK_TAKE_COUNT.fetch_add(1, Ordering::AcqRel);
    stack_pointer
}

/// Return the number of one-shot user exit return stacks stored by the entry path.
pub(super) fn user_exit_return_stack_set_count() -> u64 {
    USER_EXIT_RETURN_STACK_SET_COUNT.load(Ordering::Acquire)
}

/// Return the number of one-shot user exit return stacks consumed by `SYS_EXIT`.
pub(super) fn user_exit_return_stack_take_count() -> u64 {
    USER_EXIT_RETURN_STACK_TAKE_COUNT.load(Ordering::Acquire)
}
