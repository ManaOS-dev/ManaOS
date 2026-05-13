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
//! - [`process_timer_tick`] - Run one preemptive scheduling step
//! - [`get_current_task_id`] - Read the current task identifier

pub mod context;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use context::{TaskContext, TaskEntry};
use spin::Mutex;

const TASK_STACK_SIZE: usize = 16 * 1024;

static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);

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
    context: TaskContext,
    _stack: Option<Box<[u8]>>,
}

impl Task {
    fn bootstrap(id: u64) -> Self {
        Self {
            id,
            state: TaskState::Running,
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
            context,
            _stack: Some(stack),
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

    fn get_current_task_id(&self) -> u64 {
        self.tasks[self.current_index].get_id()
    }

    fn prepare_next_switch(&mut self) -> Option<(*mut u64, *const u64)> {
        let next_index = self.get_next_ready_index()?;
        if next_index == self.current_index {
            return None;
        }

        let current_index = self.current_index;
        self.tasks[current_index].state = TaskState::Ready;
        self.tasks[next_index].state = TaskState::Running;
        self.current_index = next_index;

        let current_context = self.tasks[current_index].context.as_mut_pointer();
        let next_context = self.tasks[next_index].context.as_pointer();
        Some((current_context, next_context))
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

/// Process one timer tick and switch to the next runnable task when possible.
pub fn process_timer_tick() {
    let switch_contexts = {
        let Some(mut scheduler) = SCHEDULER.try_lock() else {
            return;
        };

        let Some(scheduler) = scheduler.as_mut() else {
            return;
        };

        scheduler.prepare_next_switch()
    };

    let Some((current_context, next_context)) = switch_contexts else {
        return;
    };

    // SAFETY: Context pointers come from tasks stored in the scheduler. Task
    // stacks are retained by their task objects and switching occurs on one CPU.
    unsafe {
        crate::arch::x86_64::switch_context(current_context, next_context);
    }
}

/// Return the currently selected task identifier.
pub fn get_current_task_id() -> Option<u64> {
    SCHEDULER
        .try_lock()
        .and_then(|scheduler| scheduler.as_ref().map(Scheduler::get_current_task_id))
}
