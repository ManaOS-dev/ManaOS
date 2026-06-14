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
    exit_status: Option<TaskExitStatus>,
}

impl TaskMetadata {
    pub(super) const fn bootstrap() -> Self {
        Self {
            identifier: TaskIdentifier::BOOTSTRAP,
            parent_identifier: None,
            exit_status: None,
        }
    }

    pub(super) const fn child(
        identifier: TaskIdentifier,
        parent_identifier: TaskIdentifier,
    ) -> Self {
        Self {
            identifier,
            parent_identifier: Some(parent_identifier),
            exit_status: None,
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

    pub(super) fn record_exit_status(&mut self, exit_code: u64) -> bool {
        if self.exit_status.is_some() {
            return false;
        }

        self.exit_status = Some(TaskExitStatus::new(exit_code));
        true
    }

    pub(super) fn get_exit_code(&self) -> Option<u64> {
        self.exit_status.map(TaskExitStatus::exit_code)
    }

    pub(super) fn is_waitable(&self) -> bool {
        self.exit_status
            .is_some_and(|exit_status| !exit_status.wait_collected())
    }

    pub(super) fn wait_collected(&self) -> bool {
        self.exit_status.is_some_and(TaskExitStatus::wait_collected)
    }

    pub(super) fn collect_waitable_exit(&mut self) -> Option<u64> {
        let exit_status = self.exit_status.as_mut()?;
        if exit_status.wait_collected() {
            return None;
        }

        exit_status.mark_wait_collected();
        Some(exit_status.exit_code())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct TaskExitStatus {
    exit_code: u64,
    wait_collected: bool,
}

impl TaskExitStatus {
    const fn new(exit_code: u64) -> Self {
        Self {
            exit_code,
            wait_collected: false,
        }
    }

    const fn exit_code(self) -> u64 {
        self.exit_code
    }

    const fn wait_collected(self) -> bool {
        self.wait_collected
    }

    fn mark_wait_collected(&mut self) {
        self.wait_collected = true;
    }
}
