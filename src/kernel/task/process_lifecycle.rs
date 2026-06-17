//! User process lifecycle transitions.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::kernel::memory::address::VirtAddr;

static USER_RETURN_STACK: AtomicU64 = AtomicU64::new(0);
static USER_RETURN_WINDOW_TASK_ID: AtomicU64 = AtomicU64::new(0);
static USER_RETURN_STACK_SET_COUNT: AtomicU64 = AtomicU64::new(0);
static USER_RETURN_STACK_TAKE_COUNT: AtomicU64 = AtomicU64::new(0);

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

    /// Return the normal-process wait status word for this exit.
    pub fn wait_status(&self) -> u32 {
        u32::try_from((self.exit_code & 0xff) << 8)
            .expect("normal wait status word must fit in u32")
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
    let mut next_task_id = task_id;
    loop {
        run_user_task_until_kernel_return(frame_allocator, next_task_id)?;
        if let Some(exit) = reclaim_one_finished_user_task(frame_allocator) {
            return Some(exit);
        }

        next_task_id = wait_for_next_active_user_task()?;
    }
}

/// Run active user-space tasks until `task_id` blocks in `read`.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_user_task_until_read_block(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    task_id: u64,
) -> Option<()> {
    let mut next_task_id = task_id;
    loop {
        run_user_task_until_kernel_return(frame_allocator, next_task_id)?;
        if let Some(exit) = reclaim_one_finished_user_task(frame_allocator) {
            if exit.task_id() == task_id {
                return None;
            }
        }
        if super::scheduler::is_user_task_blocked_for_read(task_id) {
            return Some(());
        }

        next_task_id = wait_for_next_active_user_task()?;
    }
}

fn run_user_task_until_kernel_return(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
    task_id: u64,
) -> Option<()> {
    let mut user_task = {
        let mut scheduler = super::scheduler::SCHEDULER.lock();
        scheduler
            .as_mut()
            .expect("scheduler must be initialized before running user tasks")
            .prepare_one_shot_user_task(task_id)?
    };
    crate::kernel::memory::address_space::switch_to_user_address_space(user_task.address_space);
    super::scheduler::complete_pending_user_waitpid_status(task_id);
    if let Some(read_result) = crate::kernel::syscall::complete_pending_user_read(task_id) {
        user_task.trap_frame.rax = read_result;
    }
    super::scheduler::install_user_task_kernel_stack(user_task.kernel_stack_top);
    crate::log_info!(
        "task",
        "Installed user task kernel stack: task={} address_space={:#x} top={:#x} kernel_stack_top_typed=true architecture_stack_installer_typed=true",
        task_id,
        user_task.address_space.level_4_frame().as_u64(),
        user_task.kernel_stack_top.as_u64()
    );
    let instruction_pointer = user_task
        .trap_frame
        .instruction_pointer_address()
        .expect("user trap frame instruction pointer must be a user virtual address");
    let stack_pointer = user_task
        .trap_frame
        .stack_pointer_address()
        .expect("user trap frame stack pointer must be a user virtual address");
    crate::log_info!(
        "task",
        "User trap frame entry prepared: task={} rip={:#x} rsp={:#x} rdi={} rsi={:#x} rdx={:#x} trap_frame_user_addresses_typed=true",
        task_id,
        instruction_pointer.as_u64(),
        stack_pointer.as_u64(),
        user_task.trap_frame.rdi,
        user_task.trap_frame.rsi,
        user_task.trap_frame.rdx
    );

    begin_user_return_window(task_id);
    crate::kernel::memory::runtime_allocator::register_user_runtime_frame_allocator(
        frame_allocator,
    );
    super::scheduler::set_preemption_enabled(true);
    // SAFETY: The trap frame was derived from mapped user code and stack
    // addresses, and this restore path returns only through user stop syscalls
    // that consume the registered return stack.
    unsafe {
        super::architecture::enter_user_mode_once(user_task.trap_frame.as_pointer());
    }
    super::scheduler::set_preemption_enabled(false);
    crate::kernel::memory::runtime_allocator::clear_user_runtime_frame_allocator();
    crate::kernel::memory::address_space::switch_to_kernel_address_space();
    assert_user_return_window_consumed();
    crate::log_info!("task", "Restored kernel address space after user stop.");

    Some(())
}

fn reclaim_one_finished_user_task(
    frame_allocator: &mut crate::kernel::memory::frame_allocator::PhysicalFrameAllocator,
) -> Option<UserTaskExit> {
    let (exit, resource_reclaim) = {
        let mut scheduler = super::scheduler::SCHEDULER.lock();
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

fn wait_for_next_active_user_task() -> Option<u64> {
    loop {
        {
            let scheduler = super::scheduler::SCHEDULER.lock();
            let scheduler = scheduler
                .as_ref()
                .expect("scheduler must be initialized before waiting for active user tasks");
            if let Some(task_id) = scheduler.next_active_user_task_id() {
                return Some(task_id);
            }
            if !scheduler.has_active_user_tasks() {
                return None;
            }
        }

        core::hint::spin_loop();
    }
}

/// Mark the currently running user task as finished.
pub fn finish_current_task(exit_code: u64) -> Option<u64> {
    let exit = super::scheduler::SCHEDULER
        .lock()
        .as_mut()
        .and_then(|scheduler| scheduler.finish_current_task(exit_code))?;
    Some(exit.task_id())
}

fn begin_user_return_window(task_id: u64) {
    assert_ne!(task_id, 0, "user return window task id must be non-zero");
    assert_eq!(
        USER_RETURN_STACK.load(Ordering::Acquire),
        0,
        "user return stack must be clear before entering user mode"
    );
    USER_RETURN_WINDOW_TASK_ID
        .compare_exchange(0, task_id, Ordering::AcqRel, Ordering::Acquire)
        .expect("user return window must not already be active");
}

fn assert_user_return_window_consumed() {
    assert_eq!(
        USER_RETURN_STACK.load(Ordering::Acquire),
        0,
        "user return stack must be consumed before returning to lifecycle code"
    );
    assert_eq!(
        USER_RETURN_WINDOW_TASK_ID.load(Ordering::Acquire),
        0,
        "user return window task id must be consumed before lifecycle cleanup"
    );
}

/// Save the kernel return stack used by returnable user task entries.
#[no_mangle]
pub extern "C" fn set_user_return_stack(stack_pointer: usize) {
    let stack_pointer = user_return_stack_from_abi(stack_pointer);
    assert_ne!(
        stack_pointer.as_u64(),
        0,
        "user return stack pointer must be non-zero"
    );
    assert_ne!(
        USER_RETURN_WINDOW_TASK_ID.load(Ordering::Acquire),
        0,
        "user return window must be active before storing its stack"
    );
    assert_eq!(
        USER_RETURN_STACK.swap(stack_pointer.as_u64(), Ordering::AcqRel),
        0,
        "user return stack must be stored exactly once per entry"
    );
    USER_RETURN_STACK_SET_COUNT.fetch_add(1, Ordering::AcqRel);
}

/// Return the kernel stack pointer restored after a user stop syscall.
#[no_mangle]
pub extern "C" fn get_user_return_stack() -> usize {
    let stack_pointer = user_return_stack_from_storage(USER_RETURN_STACK.swap(0, Ordering::AcqRel));
    assert_ne!(
        stack_pointer.as_u64(),
        0,
        "user return stack must be available before returning to lifecycle code"
    );
    assert_ne!(
        USER_RETURN_WINDOW_TASK_ID.swap(0, Ordering::AcqRel),
        0,
        "user return window must be active before returning to lifecycle code"
    );
    USER_RETURN_STACK_TAKE_COUNT.fetch_add(1, Ordering::AcqRel);
    user_return_stack_to_abi(stack_pointer)
}

/// Return the number of returnable user stacks stored by the entry path.
pub(super) fn user_return_stack_set_count() -> u64 {
    USER_RETURN_STACK_SET_COUNT.load(Ordering::Acquire)
}

/// Return the number of returnable user stacks consumed after user stop syscalls.
pub(super) fn user_return_stack_take_count() -> u64 {
    USER_RETURN_STACK_TAKE_COUNT.load(Ordering::Acquire)
}

/// Verify user-return stack ABI conversion rules.
pub(crate) fn verify_typed_user_return_stack_address() -> bool {
    const REPRESENTATIVE_STACK: usize = 0xffff_8000_0000_4000;
    let representative_stack =
        u64::try_from(REPRESENTATIVE_STACK).expect("representative stack must fit in u64");
    let stack_pointer = user_return_stack_from_abi(REPRESENTATIVE_STACK);
    stack_pointer.as_u64() == representative_stack
        && user_return_stack_from_storage(stack_pointer.as_u64()) == stack_pointer
        && user_return_stack_to_abi(stack_pointer) == REPRESENTATIVE_STACK
}

fn user_return_stack_from_abi(stack_pointer: usize) -> VirtAddr {
    let stack_pointer =
        u64::try_from(stack_pointer).expect("user return stack pointer must fit in u64");
    user_return_stack_from_storage(stack_pointer)
}

fn user_return_stack_from_storage(stack_pointer: u64) -> VirtAddr {
    VirtAddr::new(stack_pointer)
}

fn user_return_stack_to_abi(stack_pointer: VirtAddr) -> usize {
    stack_pointer
        .try_as_usize()
        .expect("user return stack pointer must fit in usize")
}
