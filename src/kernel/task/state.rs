//! Task scheduler lifecycle states and transitions.

/// Current lifecycle state of a schedulable task.
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

impl TaskState {
    pub(super) fn is_ready(self) -> bool {
        self == Self::Ready
    }

    pub(super) fn prepare_to_run(&mut self) -> bool {
        if *self != Self::Ready {
            return false;
        }

        *self = Self::Running;
        true
    }

    pub(super) fn prepare_to_wait(&mut self) -> bool {
        if *self != Self::Running {
            return false;
        }

        *self = Self::Ready;
        true
    }

    pub(super) fn finish_running(&mut self) -> bool {
        if *self != Self::Running {
            return false;
        }

        *self = Self::Finished;
        true
    }
}
