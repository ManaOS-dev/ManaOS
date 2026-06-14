//! Scheduler state, task records, and public task facade functions.

use super::architecture;
use super::context::{TaskContext, TaskEntry, UserEntryArguments, UserTaskContext, UserTrapFrame};
use super::diagnostics::{
    PreemptionStateDiagnostics, SchedulerDiagnostics, SchedulerTaskSnapshot,
    TaskExitStatusDiagnostics, TaskRuntimeDiagnosticsSnapshot, TaskStateDiagnostics,
    UserHeapDiagnosticsSnapshot, UserMappingActiveDiagnosticsSnapshot,
    UserMappingLifecycleDiagnosticsSnapshot, UserVirtualMemorySnapshot,
};
use super::metadata::{TaskIdentifier, TaskMetadata};
use super::process_lifecycle::{self, UserTaskExit};
use super::reclaim::FinishedUserTaskReclaim;
use super::stack::{KernelStack, KernelStackFaultOwner, KernelStackGuardFault, KernelStackReclaim};
use super::state::TaskState;
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
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use spin::Mutex;

const USER_TASK_PREEMPTION_ENABLED: bool = true;

pub(super) static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);
static PREEMPTION_STATE: AtomicU8 = AtomicU8::new(PreemptionStateDiagnostics::Enabled.as_raw());
static USER_RETURN_PREEMPTION_WINDOW_CLOSE_COUNT: AtomicU64 = AtomicU64::new(0);

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
    mapping_total_mapped_pages: u64,
    mapping_total_released_pages: u64,
    mapping_peak_active_pages: u64,
    mapping_peak_active_records: u64,
    mapping_file_private_map_count: u64,
    sleep_wake_tick: Option<u64>,
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
            mapping_total_mapped_pages: 0,
            mapping_total_released_pages: 0,
            mapping_peak_active_pages: 0,
            mapping_peak_active_records: 0,
            mapping_file_private_map_count: 0,
            sleep_wake_tick: None,
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
    pub(super) trap_frame: UserTrapFrame,
    pub(super) kernel_stack_top: usize,
    pub(super) address_space: UserAddressSpace,
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

pub(super) struct Scheduler {
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
    one_shot_user_entry_count: u64,
    timer_user_entry_count: u64,
    user_resume_count: u64,
    user_sleep_block_count: u64,
    user_sleep_wake_count: u64,
    finished_task_count: u64,
    reclaimed_user_resource_record_count: u64,
    reclaimed_user_address_space_count: u64,
    reclaimed_user_page_count: u64,
    reclaimed_user_page_table_page_count: u64,
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
            one_shot_user_entry_count: 0,
            timer_user_entry_count: 0,
            user_resume_count: 0,
            user_sleep_block_count: 0,
            user_sleep_wake_count: 0,
            finished_task_count: 0,
            reclaimed_user_resource_record_count: 0,
            reclaimed_user_address_space_count: 0,
            reclaimed_user_page_count: 0,
            reclaimed_user_page_table_page_count: 0,
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

    fn get_current_parent_task_id(&self) -> Option<u64> {
        self.tasks[self.current_index]
            .metadata
            .get_parent_identifier()
            .map(TaskIdentifier::as_u64)
    }

    fn get_task_index(&self, task_id: u64) -> Option<usize> {
        self.tasks
            .iter()
            .position(|task| task.metadata.get_identifier().as_u64() == task_id)
    }

    fn is_user_task_active(&self, task_id: u64) -> bool {
        self.active_user_task_identifiers.contains(&task_id)
    }
}

mod diagnostics_access;
mod facade;
mod runtime;
mod user_memory;

pub(super) use facade::install_user_task_kernel_stack;
pub use facade::{
    activate_user_task, block_current_user_after_syscall, close_user_return_preemption_window,
    collect_waitable_child_exit, finish_current_task, get_current_parent_task_id,
    get_current_task_id, get_current_user_address_space, get_kernel_stack_guard_fault,
    get_kernel_stack_guard_fault_diagnostic_sample, get_scheduler_diagnostics,
    get_scheduler_task_snapshots, initialize, prepare_current_user_sleep,
    process_current_user_break, process_current_user_mapping, process_current_user_unmapping,
    process_timer_tick, record_current_user_interrupt_trap_frame, record_current_user_trap_frame,
    replace_current_user_image, run_active_user_tasks_until_empty, run_next_user_task_once,
    run_user_task_once, set_preemption_enabled, spawn, spawn_user_task,
};
