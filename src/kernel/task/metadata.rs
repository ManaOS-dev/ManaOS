//! Task identity metadata used before process ownership exists.

/// A scheduler-local task identifier.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TaskIdentifier(u64);

impl TaskIdentifier {
    /// The bootstrap kernel task identifier.
    pub const BOOTSTRAP: Self = Self(0);

    pub(super) const fn first_dynamic() -> Self {
        Self(1)
    }

    pub(super) fn allocate(&mut self) -> Self {
        let identifier = *self;
        self.0 += 1;
        identifier
    }

    /// Return this task identifier as the current syscall-facing integer.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Scheduler metadata that will become process ownership metadata later.
pub struct TaskMetadata {
    identifier: TaskIdentifier,
    parent_identifier: Option<TaskIdentifier>,
}

impl TaskMetadata {
    pub(super) const fn bootstrap() -> Self {
        Self {
            identifier: TaskIdentifier::BOOTSTRAP,
            parent_identifier: None,
        }
    }

    pub(super) const fn child(
        identifier: TaskIdentifier,
        parent_identifier: TaskIdentifier,
    ) -> Self {
        Self {
            identifier,
            parent_identifier: Some(parent_identifier),
        }
    }

    /// Return this task's scheduler-local identifier.
    pub fn get_identifier(&self) -> TaskIdentifier {
        self.identifier
    }

    /// Return the parent task identifier recorded when the task was spawned.
    pub fn get_parent_identifier(&self) -> Option<TaskIdentifier> {
        self.parent_identifier
    }
}
