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
//! - [`process_timer_tick`] - Run one preemptive scheduling step
//! - [`get_current_task_id`] - Read the current task identifier

pub mod architecture;
pub mod context;
pub mod user_mode;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use context::{TaskContext, TaskEntry, UserTaskContext};
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

const TASK_STACK_SIZE: usize = 16 * 1024;

static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);
static PREEMPTION_ENABLED: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Copy)]
enum TaskKind {
    Kernel,
    User(UserTaskContext),
}

enum SwitchAction {
    SwitchKernel {
        current_context: *mut u64,
        next_context: *const u64,
    },
    EnterUser(UserTaskContext),
}

/// Current lifecycle state of a kernel task.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TaskState {
    /// The task is ready to run.
    Ready,
    /// The task is currently running on the CPU.
    Running,
    /// The task is blocked and must not be scheduled.
    Blocked,
    /// The task has finished and must not be scheduled.
    Finished,
}

/// A schedulable kernel task.
pub struct Task {
    id: u64,
    state: TaskState,
    kind: TaskKind,
    context: TaskContext,
    _stack: Option<Box<[u8]>>,
}

impl Task {
    fn bootstrap(id: u64) -> Self {
        Self {
            id,
            state: TaskState::Running,
            kind: TaskKind::Kernel,
            context: TaskContext::new(),
            _stack: None,
        }
    }

    fn kernel(id: u64, entry: TaskEntry) -> Self {
        let mut stack = vec![0; TASK_STACK_SIZE].into_boxed_slice();
        let stack_top = stack.as_mut_ptr() as usize + stack.len();
        // SAFETY: The stack is heap allocated, writable, and retained in the
        // task object for as long as the context can be scheduled.
        let context = unsafe { TaskContext::from_stack(stack_top, entry) };

        Self {
            id,
            state: TaskState::Ready,
            kind: TaskKind::Kernel,
            context,
            _stack: Some(stack),
        }
    }

    fn user(id: u64, user_context: UserTaskContext) -> Self {
        Self {
            id,
            state: TaskState::Ready,
            kind: TaskKind::User(user_context),
            context: TaskContext::new(),
            _stack: None,
        }
    }

    /// Return this task's unique identifier.
    pub fn get_id(&self) -> u64 {
        self.id
    }
}

struct Scheduler {
    tasks: Vec<Task>,
    current_index: usize,
    next_task_id: u64,
}

impl Scheduler {
    fn new() -> Self {
        Self {
            tasks: vec![Task::bootstrap(0)],
            current_index: 0,
            next_task_id: 1,
        }
    }

    fn spawn(&mut self, entry: TaskEntry) -> u64 {
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        self.tasks.push(Task::kernel(task_id, entry));
        task_id
    }

    fn spawn_user_task(&mut self, entry_point: u64, user_stack_top: u64) -> u64 {
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        // SAFETY: The caller provides a mapped user entry point and user stack.
        let user_context = unsafe { UserTaskContext::new(entry_point, user_stack_top) };
        self.tasks.push(Task::user(task_id, user_context));
        task_id
    }

    fn get_current_task_id(&self) -> u64 {
        self.tasks[self.current_index].get_id()
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
        self.tasks[current_index].state = TaskState::Ready;
        self.tasks[next_index].state = TaskState::Running;
        self.current_index = next_index;

        let current_context = self.tasks[current_index].context.as_mut_pointer();
        if let TaskKind::User(user_context) = self.tasks[next_index].kind {
            if self.tasks[next_index].context.is_empty() {
                return Some(SwitchAction::EnterUser(user_context));
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
            if self.tasks[index].state == TaskState::Ready {
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
/// Panics if the scheduler has not been initialized.
pub fn spawn(entry: TaskEntry) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .expect("scheduler must be initialized before spawning tasks")
        .spawn(entry)
}

/// Add a runnable user-space task to the round-robin scheduler.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn spawn_user_task(entry_point: u64, user_stack_top: u64) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .expect("scheduler must be initialized before spawning user tasks")
        .spawn_user_task(entry_point, user_stack_top)
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
        SwitchAction::EnterUser(user_context) => {
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
