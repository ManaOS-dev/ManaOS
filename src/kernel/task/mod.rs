//! # `kernel::task`
//!
//! ## Owns
//! - Kernel task metadata
//! - Round-robin scheduling decisions
//! - Task context handoff requests from timer interrupts
//!
//! ## Does NOT own
//! - Architecture-specific register switching (-> arch)
//! - Timer hardware configuration (-> arch)
//!
//! ## Public API
//! - [`initialize`] - Initialize the scheduler with the bootstrap task
//! - [`spawn`] - Add a runnable kernel task
//! - [`spawn_user_task`] - Add a runnable user task
//! - [`run_user_task_once`] - Run one user task until `SYS_EXIT`
//! - [`process_timer_tick`] - Run one preemptive scheduling step
//! - [`get_current_task_id`] - Read the current task identifier

pub mod architecture;
pub mod context;
mod metadata;
pub mod process_lifecycle;
mod stack;
mod state;
pub mod user_mode;

use crate::kernel::memory::address::UserVirtualAddress;
use crate::kernel::memory::frame_allocator::BumpFrameAllocator;
use crate::kernel::memory::virtual_allocator::{
    new_dynamic_mapping_allocator, KernelVirtualRangeAllocator,
};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
pub use context::UserEntryArguments;
use context::{TaskContext, TaskEntry, UserTaskContext, UserTrapFrame};
use core::sync::atomic::{AtomicBool, Ordering};
pub use metadata::{TaskIdentifier, TaskMetadata};
use spin::Mutex;
use stack::KernelStack;
pub use state::TaskState;

const USER_TASK_PREEMPTION_ENABLED: bool = false;

static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);
static PREEMPTION_ENABLED: AtomicBool = AtomicBool::new(true);

enum TaskKind {
    Kernel,
    User(Box<UserTaskRuntime>),
}

#[derive(Debug)]
struct UserTaskRuntime {
    entry_context: UserTaskContext,
    saved_trap_frame: Option<UserTrapFrame>,
}

impl UserTaskRuntime {
    fn new(entry_context: UserTaskContext) -> Self {
        Self {
            entry_context,
            saved_trap_frame: None,
        }
    }
}

enum SwitchAction {
    SwitchKernel {
        current_context: *mut u64,
        next_context: *const u64,
    },
    EnterUser {
        context: UserTaskContext,
        kernel_stack_top: usize,
    },
}

/// Prepared one-shot user task entry state.
pub(super) struct OneShotUserTask {
    entry_context: UserTaskContext,
    kernel_stack_top: usize,
}

/// A schedulable kernel task.
pub struct Task {
    metadata: TaskMetadata,
    state: TaskState,
    kind: TaskKind,
    context: TaskContext,
    kernel_stack: Option<KernelStack>,
}

impl Task {
    fn bootstrap() -> Self {
        Self {
            metadata: TaskMetadata::bootstrap(),
            state: TaskState::Running,
            kind: TaskKind::Kernel,
            context: TaskContext::new(),
            kernel_stack: None,
        }
    }

    fn kernel(
        identifier: TaskIdentifier,
        parent_identifier: TaskIdentifier,
        entry: TaskEntry,
        frame_allocator: &mut BumpFrameAllocator,
        kernel_stack_range_allocator: &mut KernelVirtualRangeAllocator,
    ) -> Self {
        let kernel_stack = KernelStack::new_default(frame_allocator, kernel_stack_range_allocator);
        let stack_top = kernel_stack.top();
        debug_assert!(kernel_stack.base() < stack_top);
        debug_assert!(kernel_stack.byte_len() >= 16);
        debug_assert_eq!(
            kernel_stack.reserved_page_count(),
            kernel_stack.writable_page_count() + 1
        );
        // SAFETY: The stack is mapped writable, kernel-owned, and retained in
        // the task object for as long as the context can be scheduled.
        let context = unsafe { TaskContext::from_stack(stack_top, entry) };

        Self {
            metadata: TaskMetadata::child(identifier, parent_identifier),
            state: TaskState::Ready,
            kind: TaskKind::Kernel,
            context,
            kernel_stack: Some(kernel_stack),
        }
    }

    fn user(
        identifier: TaskIdentifier,
        parent_identifier: TaskIdentifier,
        user_context: UserTaskContext,
        frame_allocator: &mut BumpFrameAllocator,
        kernel_stack_range_allocator: &mut KernelVirtualRangeAllocator,
    ) -> Self {
        let kernel_stack = KernelStack::new_default(frame_allocator, kernel_stack_range_allocator);
        debug_assert!(kernel_stack.base() < kernel_stack.top());
        debug_assert!(kernel_stack.byte_len() >= 16);
        debug_assert_eq!(
            kernel_stack.reserved_page_count(),
            kernel_stack.writable_page_count() + 1
        );
        Self {
            metadata: TaskMetadata::child(identifier, parent_identifier),
            state: TaskState::Ready,
            kind: TaskKind::User(Box::new(UserTaskRuntime::new(user_context))),
            context: TaskContext::new(),
            kernel_stack: Some(kernel_stack),
        }
    }

    /// Return this task's unique identifier.
    pub fn get_id(&self) -> u64 {
        self.metadata.get_identifier().as_u64()
    }

    fn kernel_stack_byte_len(&self) -> Option<usize> {
        self.kernel_stack.as_ref().map(KernelStack::byte_len)
    }

    fn kernel_stack_top(&self) -> Option<usize> {
        self.kernel_stack.as_ref().map(KernelStack::top)
    }

    fn kernel_stack_guard_page_virtual_start(&self) -> Option<u64> {
        self.kernel_stack
            .as_ref()
            .map(KernelStack::guard_page_virtual_start)
    }

    fn kernel_stack_writable_virtual_start(&self) -> Option<u64> {
        self.kernel_stack
            .as_ref()
            .map(KernelStack::writable_virtual_start)
    }

    fn kernel_stack_virtual_top(&self) -> Option<u64> {
        self.kernel_stack.as_ref().map(KernelStack::virtual_top)
    }

    fn kernel_stack_reserved_page_count(&self) -> Option<u64> {
        self.kernel_stack
            .as_ref()
            .map(KernelStack::reserved_page_count)
    }

    fn kernel_stack_writable_page_count(&self) -> Option<u64> {
        self.kernel_stack
            .as_ref()
            .map(KernelStack::writable_page_count)
    }
}

struct Scheduler {
    tasks: Vec<Task>,
    current_index: usize,
    next_task_identifier: TaskIdentifier,
    kernel_stack_range_allocator: KernelVirtualRangeAllocator,
}

impl Scheduler {
    fn new() -> Self {
        Self {
            tasks: vec![Task::bootstrap()],
            current_index: 0,
            next_task_identifier: TaskIdentifier::first_dynamic(),
            kernel_stack_range_allocator: new_dynamic_mapping_allocator(),
        }
    }

    fn spawn(&mut self, frame_allocator: &mut BumpFrameAllocator, entry: TaskEntry) -> u64 {
        let task_identifier = self.next_task_identifier.allocate();
        let parent_identifier = self.tasks[self.current_index].metadata.get_identifier();
        let task = Task::kernel(
            task_identifier,
            parent_identifier,
            entry,
            frame_allocator,
            &mut self.kernel_stack_range_allocator,
        );
        let kernel_stack_bytes = task
            .kernel_stack_byte_len()
            .expect("kernel tasks must own a kernel stack record");
        let kernel_stack_guard_page_virtual_start = task
            .kernel_stack_guard_page_virtual_start()
            .expect("kernel tasks must own a kernel stack guard reservation");
        let kernel_stack_writable_virtual_start = task
            .kernel_stack_writable_virtual_start()
            .expect("kernel tasks must own a writable kernel stack reservation");
        let kernel_stack_virtual_top = task
            .kernel_stack_virtual_top()
            .expect("kernel tasks must own a kernel stack virtual top reservation");
        let kernel_stack_reserved_pages = task
            .kernel_stack_reserved_page_count()
            .expect("kernel tasks must own kernel stack reservation pages");
        let kernel_stack_writable_pages = task
            .kernel_stack_writable_page_count()
            .expect("kernel tasks must own kernel stack writable pages");
        debug_assert_eq!(
            task.metadata.get_parent_identifier(),
            Some(parent_identifier)
        );
        self.tasks.push(task);
        crate::log_info!(
            "task",
            "Kernel task stack prepared: task={} bytes={} guard_virtual={:#x} writable_virtual={:#x} virtual_top={:#x} reserved_pages={} writable_pages={} guard_unmapped=true writable_mapped=true",
            task_identifier.as_u64(),
            kernel_stack_bytes,
            kernel_stack_guard_page_virtual_start,
            kernel_stack_writable_virtual_start,
            kernel_stack_virtual_top,
            kernel_stack_reserved_pages,
            kernel_stack_writable_pages
        );
        task_identifier.as_u64()
    }

    fn spawn_user_task(
        &mut self,
        frame_allocator: &mut BumpFrameAllocator,
        entry_point: UserVirtualAddress,
        user_stack_top: UserVirtualAddress,
        entry_arguments: UserEntryArguments,
    ) -> u64 {
        let task_identifier = self.next_task_identifier.allocate();
        let parent_identifier = self.tasks[self.current_index].metadata.get_identifier();
        // SAFETY: The caller provides a mapped user entry point and user stack.
        let user_context =
            unsafe { UserTaskContext::new(entry_point, user_stack_top, entry_arguments) };
        let task = Task::user(
            task_identifier,
            parent_identifier,
            user_context,
            frame_allocator,
            &mut self.kernel_stack_range_allocator,
        );
        let kernel_stack_bytes = task
            .kernel_stack_byte_len()
            .expect("user tasks must own a kernel stack record");
        let kernel_stack_guard_page_virtual_start = task
            .kernel_stack_guard_page_virtual_start()
            .expect("user tasks must own a kernel stack guard reservation");
        let kernel_stack_writable_virtual_start = task
            .kernel_stack_writable_virtual_start()
            .expect("user tasks must own a writable kernel stack reservation");
        let kernel_stack_virtual_top = task
            .kernel_stack_virtual_top()
            .expect("user tasks must own a kernel stack virtual top reservation");
        let kernel_stack_reserved_pages = task
            .kernel_stack_reserved_page_count()
            .expect("user tasks must own kernel stack reservation pages");
        let kernel_stack_writable_pages = task
            .kernel_stack_writable_page_count()
            .expect("user tasks must own kernel stack writable pages");
        debug_assert_eq!(
            task.metadata.get_parent_identifier(),
            Some(parent_identifier)
        );
        self.tasks.push(task);
        crate::log_info!(
            "task",
            "User task kernel stack prepared: task={} bytes={} guard_virtual={:#x} writable_virtual={:#x} virtual_top={:#x} reserved_pages={} writable_pages={} guard_unmapped=true writable_mapped=true",
            task_identifier.as_u64(),
            kernel_stack_bytes,
            kernel_stack_guard_page_virtual_start,
            kernel_stack_writable_virtual_start,
            kernel_stack_virtual_top,
            kernel_stack_reserved_pages,
            kernel_stack_writable_pages
        );
        task_identifier.as_u64()
    }

    fn get_current_task_id(&self) -> u64 {
        self.tasks[self.current_index].get_id()
    }

    fn get_task_index(&self, task_id: u64) -> Option<usize> {
        self.tasks
            .iter()
            .position(|task| task.metadata.get_identifier().as_u64() == task_id)
    }

    fn prepare_one_shot_user_task(&mut self, task_id: u64) -> Option<OneShotUserTask> {
        let task_index = self.get_task_index(task_id)?;
        let (entry_context, kernel_stack_top) = match &self.tasks[task_index].kind {
            TaskKind::User(user_runtime) => {
                debug_assert!(
                    user_runtime.saved_trap_frame.is_none(),
                    "run-once user tasks must not have saved trap frames before preemption support"
                );
                (
                    user_runtime.entry_context,
                    self.tasks[task_index]
                        .kernel_stack_top()
                        .expect("user tasks must own a kernel stack before entry"),
                )
            }
            TaskKind::Kernel => return None,
        };

        if !self.tasks[task_index].state.is_ready() {
            return None;
        }

        self.tasks[self.current_index].state.prepare_to_wait();
        if !self.tasks[task_index].state.prepare_to_run() {
            return None;
        }
        self.current_index = task_index;
        Some(OneShotUserTask {
            entry_context,
            kernel_stack_top,
        })
    }

    fn finish_current_task(&mut self) -> Option<u64> {
        let task_id = self.tasks[self.current_index].get_id();
        if !self.tasks[self.current_index].state.finish_running() {
            return None;
        }

        if let Some(bootstrap_task) = self.tasks.first_mut() {
            bootstrap_task.state.prepare_to_run();
            self.current_index = 0;
        }

        Some(task_id)
    }

    fn prepare_next_switch(&mut self) -> Option<SwitchAction> {
        if matches!(self.tasks[self.current_index].kind, TaskKind::User(_)) {
            // TODO(phase6): switch away from user tasks after saving a full
            // user trap frame instead of the kernel-only callee-saved context.
            return None;
        }

        let next_index = self.get_next_ready_index()?;
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

        let current_context = self.tasks[current_index].context.as_mut_pointer();
        if let TaskKind::User(user_runtime) = &self.tasks[next_index].kind {
            debug_assert!(
                user_runtime.saved_trap_frame.is_none(),
                "scheduler cannot resume saved user trap frames before preemption support"
            );
            if self.tasks[next_index].context.is_empty() {
                return Some(SwitchAction::EnterUser {
                    context: user_runtime.entry_context,
                    kernel_stack_top: self.tasks[next_index]
                        .kernel_stack_top()
                        .expect("user tasks must own a kernel stack before entry"),
                });
            }
        }

        let next_context = self.tasks[next_index].context.as_pointer();
        Some(SwitchAction::SwitchKernel {
            current_context,
            next_context,
        })
    }

    fn get_next_ready_index(&self) -> Option<usize> {
        if self.tasks.len() < 2 {
            return None;
        }

        for offset in 1..=self.tasks.len() {
            let index = (self.current_index + offset) % self.tasks.len();
            if !USER_TASK_PREEMPTION_ENABLED && matches!(self.tasks[index].kind, TaskKind::User(_))
            {
                // TODO(phase7): enable this after timer interrupts save and
                // restore a full user trap frame.
                continue;
            }
            if self.tasks[index].state.is_ready() {
                return Some(index);
            }
        }

        None
    }
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
pub fn spawn(frame_allocator: &mut BumpFrameAllocator, entry: TaskEntry) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .expect("scheduler must be initialized before spawning tasks")
        .spawn(frame_allocator, entry)
}

/// Add a runnable user-space task to the round-robin scheduler.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized, kernel stack frames cannot
/// be allocated, or kernel stack page-table mapping fails.
pub fn spawn_user_task(
    frame_allocator: &mut BumpFrameAllocator,
    entry_point: UserVirtualAddress,
    user_stack_top: UserVirtualAddress,
    entry_arguments: UserEntryArguments,
) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .expect("scheduler must be initialized before spawning user tasks")
        .spawn_user_task(
            frame_allocator,
            entry_point,
            user_stack_top,
            entry_arguments,
        )
}

/// Run one user-space task until it exits through `SYS_EXIT`.
///
/// Returns the exit code reported by the user task.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_user_task_once(task_id: u64) -> Option<u64> {
    process_lifecycle::run_user_task_once(task_id)
}

/// Mark the currently running task as finished.
pub fn finish_current_task(exit_code: u64) -> Option<u64> {
    process_lifecycle::finish_current_task(exit_code)
}

/// Enable or disable timer-driven task switching.
pub fn set_preemption_enabled(enabled: bool) {
    PREEMPTION_ENABLED.store(enabled, Ordering::Release);
}

/// Process one timer tick and switch to the next runnable task when possible.
pub fn process_timer_tick() {
    if !PREEMPTION_ENABLED.load(Ordering::Acquire) {
        return;
    }

    let switch_action = {
        let Some(mut scheduler) = SCHEDULER.try_lock() else {
            return;
        };

        let Some(scheduler) = scheduler.as_mut() else {
            return;
        };

        scheduler.prepare_next_switch()
    };

    let Some(switch_action) = switch_action else {
        return;
    };

    match switch_action {
        SwitchAction::SwitchKernel {
            current_context,
            next_context,
        } => {
            // SAFETY: Context pointers come from tasks stored in the scheduler.
            // Task stacks are retained by their task objects and switching
            // occurs on one CPU.
            unsafe {
                architecture::switch_context(current_context, next_context);
            }
        }
        SwitchAction::EnterUser {
            context: user_context,
            kernel_stack_top,
        } => {
            architecture::install_kernel_stack(
                u64::try_from(kernel_stack_top).expect("kernel stack top must fit in u64"),
            );
            // SAFETY: The user task context was created from a mapped entry
            // point and stack, and the assembly stub consumes it immediately.
            unsafe {
                architecture::enter_user_mode(user_context.as_pointer());
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
