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
//! - [`run_next_user_task_once`] - Run the next active user task until one exits
//! - [`run_active_user_tasks_until_empty`] - Drain active user tasks until none remain
//! - [`UserTaskExit`] - User task exit result
//! - [`UserMappingRequest`] - Syscall-time private mapping request
//! - [`process_current_user_break`] - Process a user heap break request
//! - [`process_current_user_mapping`] - Process a private user mapping request
//! - [`process_current_user_unmapping`] - Process a private user unmapping request
//! - [`process_timer_tick`] - Run one preemptive scheduling step
//! - [`get_current_task_id`] - Read the current task identifier
//! - [`get_scheduler_diagnostics`] - Read scheduler accounting diagnostics
//! - [`get_scheduler_task_snapshots`] - Read retained task rows for diagnostics
//! - [`activate_user_task`] - Add a user task to the active scheduling set
//! - [`set_preemption_enabled`] - Enable or disable timer-driven task switching
//! - [`close_user_exit_preemption_window`] - Disable preemption after `SYS_EXIT`
//! - [`record_current_user_trap_frame`] - Save a captured user trap frame
//! - [`record_current_user_interrupt_trap_frame`] - Save a timer interrupt user trap frame
//! - [`get_kernel_stack_guard_fault`] - Classify a kernel stack guard fault
//! - [`get_kernel_stack_guard_fault_diagnostic_sample`] - Probe guard-fault diagnostics

pub mod architecture;
pub mod context;
mod diagnostics;
mod metadata;
pub mod process_lifecycle;
mod reclaim;
mod stack;
mod state;
pub mod user_mode;

use crate::kernel::memory::address::UserVirtualAddress;
use crate::kernel::memory::address_space::{self, UserAddressSpace, UserAddressSpaceReclaim};
use crate::kernel::memory::frame_allocator::PhysicalFrameAllocator;
use crate::kernel::memory::user_heap::UserHeap;
use crate::kernel::memory::user_mapping::{
    UserMappingError, UserMappingPlacement, UserMappingPlan, UserMappingSource, UserMappings,
};
use crate::kernel::memory::virtual_allocator::{
    new_dynamic_mapping_allocator, KernelVirtualRangeAllocator,
};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;
pub use context::UserEntryArguments;
use context::{TaskContext, TaskEntry, UserTaskContext, UserTrapFrame};
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
pub use diagnostics::{
    PreemptionStateDiagnostics, SchedulerDiagnostics, SchedulerTaskSnapshot, TaskKindDiagnostics,
    TaskStateDiagnostics, UserVirtualMemorySnapshot,
};
pub use metadata::{TaskIdentifier, TaskMetadata};
pub use process_lifecycle::UserTaskExit;
use reclaim::FinishedUserTaskReclaim;
use spin::Mutex;
use stack::{KernelStack, KernelStackReclaim};
pub use stack::{KernelStackFaultOwner, KernelStackGuardFault};
pub use state::TaskState;

const USER_TASK_PREEMPTION_ENABLED: bool = true;

static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);
static PREEMPTION_STATE: AtomicU8 = AtomicU8::new(PreemptionStateDiagnostics::Enabled.as_raw());
static USER_EXIT_PREEMPTION_WINDOW_CLOSE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Syscall-time private user mapping request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserMappingRequest {
    requested_address: u64,
    placement: UserMappingPlacement,
    source: UserMappingSource,
    length: u64,
    writable: bool,
    protection: u64,
    flags: u64,
}

impl UserMappingRequest {
    /// Create a private user mapping request.
    pub const fn new(
        requested_address: u64,
        placement: UserMappingPlacement,
        source: UserMappingSource,
        length: u64,
        writable: bool,
        protection: u64,
        flags: u64,
    ) -> Self {
        Self {
            requested_address,
            placement,
            source,
            length,
            writable,
            protection,
            flags,
        }
    }

    /// Return the raw requested address syscall argument.
    pub const fn requested_address(self) -> u64 {
        self.requested_address
    }

    /// Return the placement policy for this mapping.
    pub const fn placement(self) -> UserMappingPlacement {
        self.placement
    }

    /// Return the data source used to initialize this mapping.
    pub const fn source(self) -> UserMappingSource {
        self.source
    }

    /// Return the requested mapping length in bytes.
    pub const fn length(self) -> u64 {
        self.length
    }

    /// Return whether the mapping should be writable.
    pub const fn writable(self) -> bool {
        self.writable
    }

    /// Return the raw protection syscall argument.
    pub const fn protection(self) -> u64 {
        self.protection
    }

    /// Return the raw mapping flags syscall argument.
    pub const fn flags(self) -> u64 {
        self.flags
    }
}

fn current_preemption_state() -> PreemptionStateDiagnostics {
    PreemptionStateDiagnostics::from_raw(PREEMPTION_STATE.load(Ordering::Acquire))
}

enum TaskKind {
    Kernel,
    User(Box<UserTaskRuntime>),
}

impl TaskKind {
    fn kernel_stack_fault_owner(&self) -> KernelStackFaultOwner {
        match self {
            Self::Kernel => KernelStackFaultOwner::KernelTask,
            Self::User(_) => KernelStackFaultOwner::UserTask,
        }
    }
}

#[derive(Debug)]
struct UserTaskRuntime {
    address_space: Option<UserAddressSpace>,
    saved_frame: UserTrapFrame,
    heap: UserHeap,
    mappings: UserMappings,
    syscall_frame_recorded: bool,
    interrupt_frame_recorded: bool,
}

impl UserTaskRuntime {
    fn new(
        address_space: UserAddressSpace,
        entry_context: UserTaskContext,
        heap_start: UserVirtualAddress,
    ) -> Self {
        Self {
            address_space: Some(address_space),
            saved_frame: entry_context.to_trap_frame(),
            heap: UserHeap::new(heap_start),
            mappings: UserMappings::new(),
            syscall_frame_recorded: false,
            interrupt_frame_recorded: false,
        }
    }
}

#[derive(Clone, Copy)]
enum UserTrapFrameSource {
    Syscall,
    TimerInterrupt,
}

impl UserTrapFrameSource {
    fn should_log(self, user_runtime: &UserTaskRuntime) -> bool {
        match self {
            Self::Syscall => !user_runtime.syscall_frame_recorded,
            Self::TimerInterrupt => !user_runtime.interrupt_frame_recorded,
        }
    }

    fn mark_recorded(self, user_runtime: &mut UserTaskRuntime) {
        match self {
            Self::Syscall => user_runtime.syscall_frame_recorded = true,
            Self::TimerInterrupt => user_runtime.interrupt_frame_recorded = true,
        }
    }
}

enum SwitchAction {
    SwitchKernel {
        current_context: *mut u64,
        next_context: *const u64,
        next_user_kernel_stack_top: Option<usize>,
        next_user_address_space: Option<UserAddressSpace>,
    },
    EnterUser {
        current_context: *mut u64,
        task_id: u64,
        trap_frame: UserTrapFrame,
        kernel_stack_top: usize,
        address_space: UserAddressSpace,
    },
}

/// Prepared one-shot user task entry state.
pub(super) struct OneShotUserTask {
    trap_frame: UserTrapFrame,
    kernel_stack_top: usize,
    address_space: UserAddressSpace,
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
        frame_allocator: &mut PhysicalFrameAllocator,
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
        address_space: UserAddressSpace,
        user_context: UserTaskContext,
        heap_start: UserVirtualAddress,
        frame_allocator: &mut PhysicalFrameAllocator,
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
            kind: TaskKind::User(Box::new(UserTaskRuntime::new(
                address_space,
                user_context,
                heap_start,
            ))),
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

    fn kernel_stack_guard_fault(&self, fault_address: u64) -> Option<KernelStackGuardFault> {
        let kernel_stack = self.kernel_stack.as_ref()?;
        if !kernel_stack.contains_guard_address(fault_address) {
            return None;
        }

        Some(KernelStackGuardFault::new(
            self.metadata.get_identifier().as_u64(),
            self.kind.kernel_stack_fault_owner(),
            kernel_stack.guard_page_virtual_start(),
            kernel_stack.writable_virtual_start(),
            kernel_stack.virtual_top(),
        ))
    }

    fn contains_kernel_stack_writable_range(&self, start_address: u64, byte_len: u64) -> bool {
        self.kernel_stack.as_ref().is_some_and(|kernel_stack| {
            kernel_stack.contains_writable_range(start_address, byte_len)
        })
    }
}

struct Scheduler {
    tasks: Vec<Task>,
    current_index: usize,
    next_task_identifier: TaskIdentifier,
    kernel_stack_range_allocator: KernelVirtualRangeAllocator,
    active_user_task_identifiers: Vec<u64>,
    finished_user_exits: VecDeque<UserTaskExit>,
    preemption_switch_logged: bool,
    user_resume_logged: bool,
    context_switch_count: u64,
    timer_preemption_count: u64,
    user_entry_count: u64,
    user_resume_count: u64,
    finished_task_count: u64,
    reclaimed_user_resource_record_count: u64,
    reclaimed_user_kernel_stack_count: u64,
    reclaimed_user_kernel_stack_writable_pages: u64,
    reclaimed_user_kernel_stack_virtual_pages: u64,
}

impl Scheduler {
    fn new() -> Self {
        Self {
            tasks: vec![Task::bootstrap()],
            current_index: 0,
            next_task_identifier: TaskIdentifier::first_dynamic(),
            kernel_stack_range_allocator: new_dynamic_mapping_allocator(),
            active_user_task_identifiers: Vec::new(),
            finished_user_exits: VecDeque::new(),
            preemption_switch_logged: false,
            user_resume_logged: false,
            context_switch_count: 0,
            timer_preemption_count: 0,
            user_entry_count: 0,
            user_resume_count: 0,
            finished_task_count: 0,
            reclaimed_user_resource_record_count: 0,
            reclaimed_user_kernel_stack_count: 0,
            reclaimed_user_kernel_stack_writable_pages: 0,
            reclaimed_user_kernel_stack_virtual_pages: 0,
        }
    }

    fn spawn(&mut self, frame_allocator: &mut PhysicalFrameAllocator, entry: TaskEntry) -> u64 {
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
        frame_allocator: &mut PhysicalFrameAllocator,
        address_space: UserAddressSpace,
        entry_point: UserVirtualAddress,
        user_stack_top: UserVirtualAddress,
        heap_start: UserVirtualAddress,
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
            address_space,
            user_context,
            heap_start,
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
            "User task kernel stack prepared: task={} address_space={:#x} heap_start={:#x} bytes={} guard_virtual={:#x} writable_virtual={:#x} virtual_top={:#x} reserved_pages={} writable_pages={} guard_unmapped=true writable_mapped=true",
            task_identifier.as_u64(),
            address_space.level_4_frame().as_u64(),
            heap_start.as_u64(),
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

    fn get_diagnostics(&self) -> SchedulerDiagnostics {
        let mut ready_tasks = 0_u64;
        let mut running_tasks = 0_u64;
        let mut blocked_tasks = 0_u64;
        let mut finished_tasks = 0_u64;
        let mut kernel_tasks = 0_u64;
        let mut user_tasks = 0_u64;
        let mut active_user_address_spaces = 0_u64;

        for task in &self.tasks {
            match task.state {
                TaskState::Ready => ready_tasks = ready_tasks.saturating_add(1),
                TaskState::Running => running_tasks = running_tasks.saturating_add(1),
                TaskState::Blocked => blocked_tasks = blocked_tasks.saturating_add(1),
                TaskState::Finished => finished_tasks = finished_tasks.saturating_add(1),
            }
            match &task.kind {
                TaskKind::Kernel => kernel_tasks = kernel_tasks.saturating_add(1),
                TaskKind::User(user_runtime) => {
                    user_tasks = user_tasks.saturating_add(1);
                    if user_runtime.address_space.is_some() {
                        active_user_address_spaces = active_user_address_spaces.saturating_add(1);
                    }
                }
            }
        }

        SchedulerDiagnostics {
            total_tasks: u64::try_from(self.tasks.len()).expect("task count must fit in u64"),
            kernel_tasks,
            user_tasks,
            active_user_tasks: u64::try_from(self.active_user_task_identifiers.len())
                .expect("active user task count must fit in u64"),
            active_user_address_spaces,
            states: TaskStateDiagnostics::new(
                ready_tasks,
                running_tasks,
                blocked_tasks,
                finished_tasks,
            ),
            context_switches: self.context_switch_count,
            timer_preemptions: self.timer_preemption_count,
            user_entries: self.user_entry_count,
            user_resumes: self.user_resume_count,
            finished_tasks: self.finished_task_count,
            pending_user_exits: u64::try_from(self.finished_user_exits.len())
                .expect("pending user exit count must fit in u64"),
            preemption_state: current_preemption_state(),
            user_exit_preemption_window_closes: USER_EXIT_PREEMPTION_WINDOW_CLOSE_COUNT
                .load(Ordering::Acquire),
            user_exit_return_stack_sets: process_lifecycle::user_exit_return_stack_set_count(),
            user_exit_return_stack_takes: process_lifecycle::user_exit_return_stack_take_count(),
            reclaimed_user_resource_records: self.reclaimed_user_resource_record_count,
            reclaimed_user_kernel_stacks: self.reclaimed_user_kernel_stack_count,
            reclaimed_user_kernel_stack_writable_pages: self
                .reclaimed_user_kernel_stack_writable_pages,
            reclaimed_user_kernel_stack_virtual_pages: self
                .reclaimed_user_kernel_stack_virtual_pages,
        }
    }

    fn get_task_snapshots(&self) -> Vec<SchedulerTaskSnapshot> {
        self.tasks
            .iter()
            .map(|task| {
                let task_id = task.get_id();
                let parent_task_id = task
                    .metadata
                    .get_parent_identifier()
                    .map(TaskIdentifier::as_u64);
                let active = self.is_user_task_active(task_id);
                match &task.kind {
                    TaskKind::Kernel => SchedulerTaskSnapshot::new_kernel(
                        task_id,
                        parent_task_id,
                        task.state,
                        active,
                        task.kernel_stack.is_some(),
                    ),
                    TaskKind::User(user_runtime) => {
                        let user_virtual_memory = UserVirtualMemorySnapshot::new(
                            user_runtime.heap.base().as_u64(),
                            user_runtime.heap.current_break().as_u64(),
                            user_runtime.heap.mapped_pages(),
                            user_runtime.mappings.next_start(),
                            user_runtime.mappings.active_pages(),
                            user_runtime.mappings.active_records(),
                            user_runtime.mappings.active_file_private_records(),
                        );
                        SchedulerTaskSnapshot::new_user(
                            task_id,
                            parent_task_id,
                            task.state,
                            active,
                            user_runtime.address_space.is_some(),
                            task.kernel_stack.is_some(),
                            user_virtual_memory,
                        )
                    }
                }
            })
            .collect()
    }

    fn get_kernel_stack_guard_fault(&self, fault_address: u64) -> Option<KernelStackGuardFault> {
        self.tasks
            .iter()
            .find_map(|task| task.kernel_stack_guard_fault(fault_address))
    }

    fn get_kernel_stack_guard_fault_diagnostic_sample(&self) -> Option<KernelStackGuardFault> {
        let sample_guard_address = self
            .tasks
            .iter()
            .find_map(Task::kernel_stack_guard_page_virtual_start)?;
        self.get_kernel_stack_guard_fault(sample_guard_address)
    }

    fn get_task_index(&self, task_id: u64) -> Option<usize> {
        self.tasks
            .iter()
            .position(|task| task.metadata.get_identifier().as_u64() == task_id)
    }

    fn is_user_task_active(&self, task_id: u64) -> bool {
        self.active_user_task_identifiers.contains(&task_id)
    }

    fn activate_user_task(&mut self, task_id: u64) -> bool {
        let Some(task_index) = self.get_task_index(task_id) else {
            return false;
        };
        let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
            return false;
        };
        if user_runtime.address_space.is_none()
            || self.tasks[task_index].state == TaskState::Finished
        {
            return false;
        }
        if !self.is_user_task_active(task_id) {
            self.active_user_task_identifiers.push(task_id);
        }
        true
    }

    fn next_active_user_task_id(&self) -> Option<u64> {
        self.active_user_task_identifiers
            .iter()
            .copied()
            .find(|task_id| {
                let Some(task_index) = self.get_task_index(*task_id) else {
                    return false;
                };
                let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
                    return false;
                };
                self.tasks[task_index].state.is_ready() && user_runtime.address_space.is_some()
            })
    }

    fn deactivate_user_task(&mut self, task_id: u64) {
        self.active_user_task_identifiers
            .retain(|active_task_id| *active_task_id != task_id);
    }

    fn prepare_one_shot_user_task(&mut self, task_id: u64) -> Option<OneShotUserTask> {
        let task_index = self.get_task_index(task_id)?;
        let TaskKind::User(user_runtime) = &self.tasks[task_index].kind else {
            return None;
        };
        let address_space = user_runtime.address_space?;
        let trap_frame = user_runtime.saved_frame;
        let kernel_stack_top = self.tasks[task_index]
            .kernel_stack_top()
            .expect("user tasks must own a kernel stack before entry");

        if !self.tasks[task_index].state.is_ready() {
            return None;
        }

        if !self.tasks[self.current_index].state.prepare_to_block() {
            return None;
        }
        if !self.tasks[task_index].state.prepare_to_run() {
            self.tasks[self.current_index].state.resume_blocked();
            return None;
        }
        self.current_index = task_index;
        self.activate_user_task(task_id);
        self.user_entry_count = self.user_entry_count.saturating_add(1);
        Some(OneShotUserTask {
            trap_frame,
            kernel_stack_top,
            address_space,
        })
    }

    fn finish_current_task(&mut self, exit_code: u64) -> Option<UserTaskExit> {
        let task_id = self.tasks[self.current_index].get_id();
        if !matches!(&self.tasks[self.current_index].kind, TaskKind::User(_)) {
            return None;
        }
        if !self.tasks[self.current_index].state.finish_running() {
            return None;
        }

        if let Some(bootstrap_task) = self.tasks.first_mut() {
            if !bootstrap_task.state.resume_blocked() {
                bootstrap_task.state.prepare_to_run();
            }
            self.current_index = 0;
        }
        self.deactivate_user_task(task_id);
        self.finished_task_count = self.finished_task_count.saturating_add(1);

        let exit = UserTaskExit::new(task_id, exit_code);
        self.finished_user_exits.push_back(exit);
        Some(exit)
    }

    fn take_finished_user_exit(&mut self) -> Option<UserTaskExit> {
        self.finished_user_exits.pop_front()
    }

    fn process_current_user_break(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        requested_break: u64,
    ) -> Option<u64> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return None;
        };
        let address_space = user_runtime.address_space?;
        let previous_break = user_runtime.heap.current_break();
        let next_break =
            user_runtime
                .heap
                .process_break(address_space, frame_allocator, requested_break);
        crate::log_info!(
            "syscall",
            "brk -> task={} requested={:#x} heap_base={:#x} previous={:#x} next={:#x} mapped_end={:#x} mapped_pages={}",
            task_id,
            requested_break,
            user_runtime.heap.base().as_u64(),
            previous_break.as_u64(),
            next_break.as_u64(),
            user_runtime.heap.mapped_end().as_u64(),
            user_runtime.heap.mapped_pages()
        );
        Some(next_break.as_u64())
    }

    fn process_current_user_mapping(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        request: UserMappingRequest,
        initialize_page: impl FnMut(u64, &mut [u8]) -> Result<(), UserMappingError>,
    ) -> Result<u64, UserMappingError> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return Err(UserMappingError::InvalidRequest);
        };
        let address_space = user_runtime
            .address_space
            .ok_or(UserMappingError::InvalidRequest)?;
        let allocation = user_runtime.mappings.map_private(
            address_space,
            frame_allocator,
            UserMappingPlan::new(
                request.placement(),
                request.length(),
                request.writable(),
                request.source(),
            ),
            initialize_page,
        )?;
        crate::log_info!(
            "syscall",
            "mmap -> task={} requested={:#x} start={:#x} length={} pages={} protection={:#x} flags={:#x} placement={} source={} active_pages={} file_private_records={}",
            task_id,
            request.requested_address(),
            allocation.start().as_u64(),
            request.length(),
            allocation.page_count(),
            request.protection(),
            request.flags(),
            request.placement().as_str(),
            request.source().as_str(),
            user_runtime.mappings.active_pages(),
            user_runtime.mappings.active_file_private_records()
        );
        Ok(allocation.start().as_u64())
    }

    fn process_current_user_unmapping(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        start_address: u64,
        length: u64,
    ) -> Option<u64> {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return None;
        };
        let address_space = user_runtime.address_space?;
        let unmapped_pages = user_runtime.mappings.unmap_range(
            address_space,
            frame_allocator,
            start_address,
            length,
        )?;
        crate::log_info!(
            "syscall",
            "munmap -> task={} start={:#x} length={} pages={} unmapped=true active_pages={} active_records={}",
            task_id,
            start_address,
            length,
            unmapped_pages,
            user_runtime.mappings.active_pages(),
            user_runtime.mappings.active_records()
        );
        Some(unmapped_pages)
    }

    fn reclaim_finished_user_resources(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        task_id: u64,
    ) -> Option<FinishedUserTaskReclaim> {
        let task_index = self.get_task_index(task_id)?;
        if self.tasks[task_index].state != TaskState::Finished {
            return None;
        }
        if !matches!(&self.tasks[task_index].kind, TaskKind::User(_)) {
            return None;
        }

        let address_space_reclaim =
            self.reclaim_finished_user_address_space_at_index(frame_allocator, task_index);
        let kernel_stack_reclaim =
            self.reclaim_finished_user_kernel_stack_at_index(frame_allocator, task_index);
        let reclaim = FinishedUserTaskReclaim::new(address_space_reclaim, kernel_stack_reclaim);
        if reclaim.reclaimed_anything() {
            self.reclaimed_user_resource_record_count =
                self.reclaimed_user_resource_record_count.saturating_add(1);
        }
        Some(reclaim)
    }

    fn reclaim_finished_user_address_space_at_index(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        task_index: usize,
    ) -> Option<UserAddressSpaceReclaim> {
        let TaskKind::User(user_runtime) = &mut self.tasks[task_index].kind else {
            return None;
        };
        let address_space = user_runtime.address_space.take()?;
        Some(address_space::destroy_user_address_space(
            frame_allocator,
            address_space,
        ))
    }

    fn reclaim_finished_user_kernel_stack_at_index(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        task_index: usize,
    ) -> Option<KernelStackReclaim> {
        let kernel_stack = self.tasks[task_index].kernel_stack.take()?;
        let reclaim = kernel_stack.destroy(frame_allocator, &mut self.kernel_stack_range_allocator);
        self.reclaimed_user_kernel_stack_count =
            self.reclaimed_user_kernel_stack_count.saturating_add(1);
        self.reclaimed_user_kernel_stack_writable_pages = self
            .reclaimed_user_kernel_stack_writable_pages
            .saturating_add(reclaim.writable_pages());
        self.reclaimed_user_kernel_stack_virtual_pages = self
            .reclaimed_user_kernel_stack_virtual_pages
            .saturating_add(reclaim.virtual_pages());
        Some(reclaim)
    }

    fn record_current_user_trap_frame(
        &mut self,
        trap_frame: UserTrapFrame,
        trap_frame_storage_address: u64,
        source: UserTrapFrameSource,
    ) {
        let current_task = &mut self.tasks[self.current_index];
        let task_id = current_task.get_id();
        let trap_frame_byte_len = u64::try_from(core::mem::size_of::<UserTrapFrame>())
            .expect("user trap frame size must fit in u64");
        let trap_frame_on_kernel_stack = current_task
            .contains_kernel_stack_writable_range(trap_frame_storage_address, trap_frame_byte_len);
        let TaskKind::User(user_runtime) = &mut current_task.kind else {
            return;
        };

        let should_log = source.should_log(user_runtime);
        user_runtime.saved_frame = trap_frame;
        source.mark_recorded(user_runtime);

        if !should_log {
            return;
        }

        match source {
            UserTrapFrameSource::Syscall => crate::log_info!(
                "task",
                "User syscall trap frame saved: task={} frame_storage={:#x} on_kernel_stack={} rip={:#x} rsp={:#x} rax={:#x} rdi={:#x} rsi={:#x} rdx={:#x} r10={:#x} r8={:#x} r9={:#x}",
                task_id,
                trap_frame_storage_address,
                trap_frame_on_kernel_stack,
                user_runtime.saved_frame.instruction_pointer,
                user_runtime.saved_frame.stack_pointer,
                user_runtime.saved_frame.rax,
                user_runtime.saved_frame.rdi,
                user_runtime.saved_frame.rsi,
                user_runtime.saved_frame.rdx,
                user_runtime.saved_frame.r10,
                user_runtime.saved_frame.r8,
                user_runtime.saved_frame.r9
            ),
            UserTrapFrameSource::TimerInterrupt => crate::log_info!(
                "task",
                "User timer trap frame saved: task={} frame_storage={:#x} on_kernel_stack={} rip={:#x} rsp={:#x} rax={:#x} rcx={:#x} r11={:#x}",
                task_id,
                trap_frame_storage_address,
                trap_frame_on_kernel_stack,
                user_runtime.saved_frame.instruction_pointer,
                user_runtime.saved_frame.stack_pointer,
                user_runtime.saved_frame.rax,
                user_runtime.saved_frame.rcx,
                user_runtime.saved_frame.r11
            ),
        }
    }

    fn can_switch_current_task_away(&self, interrupted_user_mode: bool) -> bool {
        match &self.tasks[self.current_index].kind {
            TaskKind::Kernel => true,
            TaskKind::User(user_runtime) => {
                interrupted_user_mode && user_runtime.interrupt_frame_recorded
            }
        }
    }

    fn can_schedule_task(&self, current_index: usize, candidate_index: usize) -> bool {
        if !self.tasks[candidate_index].state.is_ready() {
            return false;
        }

        match &self.tasks[candidate_index].kind {
            TaskKind::Kernel => !self.tasks[candidate_index].context.is_empty(),
            TaskKind::User(_) => {
                if !USER_TASK_PREEMPTION_ENABLED
                    || !self.is_user_task_active(self.tasks[candidate_index].get_id())
                {
                    return false;
                }

                let current_is_user = matches!(self.tasks[current_index].kind, TaskKind::User(_));
                let candidate_needs_first_entry = self.tasks[candidate_index].context.is_empty();
                !(current_is_user && candidate_needs_first_entry)
            }
        }
    }

    fn user_kernel_stack_top(&self, index: usize) -> Option<usize> {
        match &self.tasks[index].kind {
            TaskKind::User(_) => Some(
                self.tasks[index]
                    .kernel_stack_top()
                    .expect("user tasks must own a kernel stack before entry or resume"),
            ),
            TaskKind::Kernel => None,
        }
    }

    fn user_address_space(&self, index: usize) -> Option<UserAddressSpace> {
        match &self.tasks[index].kind {
            TaskKind::User(user_runtime) => user_runtime.address_space,
            TaskKind::Kernel => None,
        }
    }

    fn is_first_entry_user_candidate(&self, index: usize) -> bool {
        matches!(self.tasks[index].kind, TaskKind::User(_))
            && self.tasks[index].context.is_empty()
            && self.is_user_task_active(self.tasks[index].get_id())
    }

    fn prepare_next_switch(&mut self, interrupted_user_mode: bool) -> Option<SwitchAction> {
        if !self.can_switch_current_task_away(interrupted_user_mode) {
            return None;
        }

        let next_index = self.get_next_ready_index(self.current_index)?;
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
        self.context_switch_count = self.context_switch_count.saturating_add(1);

        let current_task_id = self.tasks[current_index].get_id();
        let next_task_id = self.tasks[next_index].get_id();
        if matches!(self.tasks[current_index].kind, TaskKind::User(_)) {
            self.timer_preemption_count = self.timer_preemption_count.saturating_add(1);
            if !self.preemption_switch_logged {
                crate::log_info!(
                    "task",
                    "User task preempted by timer: current={} next={} context_saved=true",
                    current_task_id,
                    next_task_id
                );
                self.preemption_switch_logged = true;
            }
        }

        let current_context = self.tasks[current_index].context.as_mut_pointer();
        let next_user_kernel_stack_top = self.user_kernel_stack_top(next_index);
        let next_user_address_space = self.user_address_space(next_index);
        if let TaskKind::User(user_runtime) = &self.tasks[next_index].kind {
            if self.tasks[next_index].context.is_empty() {
                self.user_entry_count = self.user_entry_count.saturating_add(1);
                return Some(SwitchAction::EnterUser {
                    current_context,
                    task_id: next_task_id,
                    trap_frame: user_runtime.saved_frame,
                    kernel_stack_top: next_user_kernel_stack_top
                        .expect("user tasks must own a kernel stack before entry"),
                    address_space: next_user_address_space
                        .expect("user tasks must own an address space before entry"),
                });
            }
            if !self.user_resume_logged {
                crate::log_info!(
                    "task",
                    "User task resumed from timer context: task={} kernel_stack_top={:#x}",
                    next_task_id,
                    next_user_kernel_stack_top
                        .expect("user tasks must own a kernel stack before resume")
                );
                self.user_resume_logged = true;
            }
            self.user_resume_count = self.user_resume_count.saturating_add(1);
        }

        let next_context = self.tasks[next_index].context.as_pointer();
        Some(SwitchAction::SwitchKernel {
            current_context,
            next_context,
            next_user_kernel_stack_top,
            next_user_address_space,
        })
    }

    fn get_next_ready_index(&self, current_index: usize) -> Option<usize> {
        if self.tasks.len() < 2 {
            return None;
        }

        if matches!(self.tasks[current_index].kind, TaskKind::Kernel) {
            for offset in 1..=self.tasks.len() {
                let index = (current_index + offset) % self.tasks.len();
                if self.can_schedule_task(current_index, index)
                    && self.is_first_entry_user_candidate(index)
                {
                    return Some(index);
                }
            }
        }

        for offset in 1..=self.tasks.len() {
            let index = (current_index + offset) % self.tasks.len();
            if self.can_schedule_task(current_index, index) {
                return Some(index);
            }
        }

        None
    }
}

pub(super) fn install_user_task_kernel_stack(kernel_stack_top: usize) {
    let kernel_stack_top =
        u64::try_from(kernel_stack_top).expect("kernel stack top must fit in u64");
    architecture::install_kernel_stack(kernel_stack_top);
    crate::kernel::interrupt::set_syscall_kernel_stack_top(kernel_stack_top);
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
pub fn spawn(frame_allocator: &mut PhysicalFrameAllocator, entry: TaskEntry) -> u64 {
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
    frame_allocator: &mut PhysicalFrameAllocator,
    address_space: UserAddressSpace,
    entry_point: UserVirtualAddress,
    user_stack_top: UserVirtualAddress,
    heap_start: UserVirtualAddress,
    entry_arguments: UserEntryArguments,
) -> u64 {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .expect("scheduler must be initialized before spawning user tasks")
        .spawn_user_task(
            frame_allocator,
            address_space,
            entry_point,
            user_stack_top,
            heap_start,
            entry_arguments,
        )
}

/// Add a user task to the active scheduling set.
pub fn activate_user_task(task_id: u64) -> bool {
    let mut scheduler = SCHEDULER.lock();
    scheduler
        .as_mut()
        .is_some_and(|scheduler| scheduler.activate_user_task(task_id))
}

/// Run active user-space tasks until one exits through `SYS_EXIT`.
///
/// Starts with `task_id` and returns the exit reported by the active user task
/// that reached `SYS_EXIT`.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_user_task_once(
    frame_allocator: &mut PhysicalFrameAllocator,
    task_id: u64,
) -> Option<UserTaskExit> {
    process_lifecycle::run_user_task_once(frame_allocator, task_id)
}

/// Run the next active user task until one active user task exits.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_next_user_task_once(
    frame_allocator: &mut PhysicalFrameAllocator,
) -> Option<UserTaskExit> {
    let task_id = {
        let scheduler = SCHEDULER.lock();
        scheduler
            .as_ref()
            .expect("scheduler must be initialized before running active user tasks")
            .next_active_user_task_id()?
    };
    run_user_task_once(frame_allocator, task_id)
}

/// Run active user tasks until no runnable active user task remains.
///
/// Returns one exit record for each active user task that exited.
///
/// # Panics
///
/// Panics if the scheduler has not been initialized.
pub fn run_active_user_tasks_until_empty(
    frame_allocator: &mut PhysicalFrameAllocator,
) -> Vec<UserTaskExit> {
    let mut exits = Vec::new();
    while let Some(exit) = run_next_user_task_once(frame_allocator) {
        exits.push(exit);
    }
    crate::log_info!(
        "task",
        "Active user lifecycle drained: exits={}",
        exits.len()
    );
    exits
}

/// Mark the currently running task as finished.
pub fn finish_current_task(exit_code: u64) -> Option<u64> {
    process_lifecycle::finish_current_task(exit_code)
}

/// Process a `brk` request for the currently running user task.
pub fn process_current_user_break(
    frame_allocator: &mut PhysicalFrameAllocator,
    requested_break: u64,
) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().and_then(|scheduler| {
        scheduler.process_current_user_break(frame_allocator, requested_break)
    })
}

/// Process a private `mmap` request for the currently running user task.
pub fn process_current_user_mapping(
    frame_allocator: &mut PhysicalFrameAllocator,
    request: UserMappingRequest,
    initialize_page: impl FnMut(u64, &mut [u8]) -> Result<(), UserMappingError>,
) -> Option<Result<u64, UserMappingError>> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().map(|scheduler| {
        scheduler.process_current_user_mapping(frame_allocator, request, initialize_page)
    })
}

/// Process a private `munmap` request for the currently running user task.
pub fn process_current_user_unmapping(
    frame_allocator: &mut PhysicalFrameAllocator,
    start_address: u64,
    length: u64,
) -> Option<u64> {
    let mut scheduler = SCHEDULER.lock();
    scheduler.as_mut().and_then(|scheduler| {
        scheduler.process_current_user_unmapping(frame_allocator, start_address, length)
    })
}

/// Save a captured user trap frame for the currently running user task.
pub fn record_current_user_trap_frame(trap_frame: UserTrapFrame, trap_frame_storage_address: u64) {
    let mut scheduler = SCHEDULER.lock();
    if let Some(scheduler) = scheduler.as_mut() {
        scheduler.record_current_user_trap_frame(
            trap_frame,
            trap_frame_storage_address,
            UserTrapFrameSource::Syscall,
        );
    }
}

/// Save a timer-interrupt user trap frame for the currently running user task.
pub fn record_current_user_interrupt_trap_frame(
    trap_frame: UserTrapFrame,
    trap_frame_storage_address: u64,
) {
    let mut scheduler = SCHEDULER.lock();
    if let Some(scheduler) = scheduler.as_mut() {
        scheduler.record_current_user_trap_frame(
            trap_frame,
            trap_frame_storage_address,
            UserTrapFrameSource::TimerInterrupt,
        );
    }
}

/// Enable or disable timer-driven task switching.
pub fn set_preemption_enabled(enabled: bool) {
    let state = if enabled {
        PreemptionStateDiagnostics::Enabled
    } else {
        PreemptionStateDiagnostics::Disabled
    };
    PREEMPTION_STATE.store(state.as_raw(), Ordering::Release);
}

/// Disable timer-driven task switching after a user task exits through `SYS_EXIT`.
pub fn close_user_exit_preemption_window(task_id: u64) {
    PREEMPTION_STATE.store(
        PreemptionStateDiagnostics::UserExitReturn.as_raw(),
        Ordering::Release,
    );
    USER_EXIT_PREEMPTION_WINDOW_CLOSE_COUNT.fetch_add(1, Ordering::AcqRel);
    crate::log_info!(
        "task",
        "User exit preemption window closed: task={}",
        task_id
    );
}

/// Process one timer tick and switch to the next runnable task when possible.
pub fn process_timer_tick(interrupted_user_mode: bool) {
    if !current_preemption_state().is_enabled() {
        return;
    }

    let switch_action = {
        let Some(mut scheduler) = SCHEDULER.try_lock() else {
            return;
        };

        let Some(scheduler) = scheduler.as_mut() else {
            return;
        };

        scheduler.prepare_next_switch(interrupted_user_mode)
    };

    let Some(switch_action) = switch_action else {
        return;
    };

    match switch_action {
        SwitchAction::SwitchKernel {
            current_context,
            next_context,
            next_user_kernel_stack_top,
            next_user_address_space,
        } => {
            if let Some(address_space) = next_user_address_space {
                address_space::switch_to_user_address_space(address_space);
            } else {
                address_space::switch_to_kernel_address_space();
            }
            if let Some(kernel_stack_top) = next_user_kernel_stack_top {
                install_user_task_kernel_stack(kernel_stack_top);
            }
            // SAFETY: Context pointers come from tasks stored in the scheduler.
            // Task stacks are retained by their task objects and switching
            // occurs on one CPU.
            unsafe {
                architecture::switch_context(current_context, next_context);
            }
        }
        SwitchAction::EnterUser {
            current_context,
            task_id,
            trap_frame,
            kernel_stack_top,
            address_space,
        } => {
            address_space::switch_to_user_address_space(address_space);
            install_user_task_kernel_stack(kernel_stack_top);
            crate::log_info!(
                "task",
                "User task entered from timer context: task={} address_space={:#x} kernel_stack_top={:#x}",
                task_id,
                address_space.level_4_frame().as_u64(),
                kernel_stack_top
            );
            // SAFETY: The current context pointer and user trap frame come
            // from tasks stored in the scheduler. The assembly entry saves the
            // current context before consuming the user frame.
            unsafe {
                architecture::switch_to_user_mode(current_context, trap_frame.as_pointer());
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

/// Return scheduler task counts and lifecycle accounting diagnostics.
pub fn get_scheduler_diagnostics() -> Option<SchedulerDiagnostics> {
    SCHEDULER
        .try_lock()
        .and_then(|scheduler| scheduler.as_ref().map(Scheduler::get_diagnostics))
}

/// Return one snapshot row for each task retained by the scheduler.
pub fn get_scheduler_task_snapshots() -> Option<Vec<SchedulerTaskSnapshot>> {
    SCHEDULER
        .try_lock()
        .and_then(|scheduler| scheduler.as_ref().map(Scheduler::get_task_snapshots))
}

/// Return guard-fault diagnostics when `fault_address` is inside a known kernel
/// stack guard page.
pub fn get_kernel_stack_guard_fault(fault_address: u64) -> Option<KernelStackGuardFault> {
    SCHEDULER.try_lock().and_then(|scheduler| {
        scheduler
            .as_ref()
            .and_then(|scheduler| scheduler.get_kernel_stack_guard_fault(fault_address))
    })
}

/// Return a representative guard-fault diagnostic sample for boot-time checks.
pub fn get_kernel_stack_guard_fault_diagnostic_sample() -> Option<KernelStackGuardFault> {
    SCHEDULER.try_lock().and_then(|scheduler| {
        scheduler
            .as_ref()
            .and_then(Scheduler::get_kernel_stack_guard_fault_diagnostic_sample)
    })
}
