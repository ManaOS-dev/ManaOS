//! Scheduler state, task records, and public task facade functions.

use super::architecture;
use super::context::{TaskContext, TaskEntry, UserEntryArguments, UserTaskContext, UserTrapFrame};
use super::diagnostics::{
    PreemptionStateDiagnostics, SchedulerDiagnostics, SchedulerTaskSnapshot,
    TaskExitStatusDiagnostics, TaskRuntimeDiagnosticsSnapshot, TaskStateDiagnostics,
    TaskStatusDiagnosticsSnapshot, UserExecveDiagnosticsSnapshot,
    UserExecveReplacementStateDiagnostics, UserHeapDiagnosticsSnapshot,
    UserImageDiagnosticsSnapshot, UserMappingActiveDiagnosticsSnapshot,
    UserMappingLifecycleDiagnosticsSnapshot, UserPreemptionReasonDiagnostics,
    UserResumePathDiagnostics, UserTrapFrameDiagnosticsSnapshot, UserVirtualMemorySnapshot,
    USER_IMAGE_PATH_DIAGNOSTIC_BYTES,
};
use super::metadata::{TaskIdentifier, TaskMetadata};
use super::process_lifecycle::{self, UserTaskExit};
use super::reclaim::FinishedUserTaskReclaim;
use super::stack::{KernelStack, KernelStackFaultOwner, KernelStackGuardFault, KernelStackReclaim};
use super::state::TaskState;
use crate::kernel::filesystem::{FileDescriptorTable, SpawnDescriptorInheritanceSnapshot};
use crate::kernel::memory::address::{
    PhysicalFrameStart, UserVirtualAddress, UserWritableRange, VirtAddr,
};
use crate::kernel::memory::address_space::{self, UserAddressSpace, UserAddressSpaceReclaim};
use crate::kernel::memory::frame_allocator::PhysicalFrameAllocator;
use crate::kernel::memory::user_heap::UserHeap;
use crate::kernel::memory::user_mapping::{
    UserMappingError, UserMappingPlacement, UserMappingPlan, UserMappingSource,
    UserMappingUnmapRequest, UserMappings,
};
use crate::kernel::memory::virtual_allocator::{
    new_dynamic_mapping_allocator, KernelVirtualRangeAllocator,
};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::ToString;
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

/// Pending user read syscall state that waits for device input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserReadRequest {
    file_descriptor: usize,
    user_buffer: UserWritableRange,
}

impl UserReadRequest {
    /// Create a pending user read request.
    pub const fn new(file_descriptor: usize, user_buffer: UserWritableRange) -> Self {
        Self {
            file_descriptor,
            user_buffer,
        }
    }

    /// Return the file descriptor being read.
    pub const fn file_descriptor(self) -> usize {
        self.file_descriptor
    }

    /// Return the destination user buffer.
    pub const fn user_buffer(self) -> UserWritableRange {
        self.user_buffer
    }

    /// Return the requested byte length.
    pub const fn byte_len(self) -> usize {
        self.user_buffer.as_range().byte_len()
    }
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

/// Scheduler-internal request for constructing a user task record.
#[derive(Clone, Copy)]
pub(in crate::kernel::task::scheduler) struct UserTaskSpawnRequest<'a> {
    address_space: UserAddressSpace,
    entry_point: UserVirtualAddress,
    user_stack_top: UserVirtualAddress,
    heap_start: UserVirtualAddress,
    entry_arguments: UserEntryArguments,
    spawn_origin_path: &'a str,
}

impl<'a> UserTaskSpawnRequest<'a> {
    /// Create a scheduler-internal user task spawn request.
    pub(in crate::kernel::task::scheduler) const fn new(
        address_space: UserAddressSpace,
        entry_point: UserVirtualAddress,
        user_stack_top: UserVirtualAddress,
        heap_start: UserVirtualAddress,
        entry_arguments: UserEntryArguments,
        spawn_origin_path: &'a str,
    ) -> Self {
        Self {
            address_space,
            entry_point,
            user_stack_top,
            heap_start,
            entry_arguments,
            spawn_origin_path,
        }
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
    image: UserImageRuntime,
    heap: UserHeap,
    mappings: UserMappings,
    mapping_total_mapped_pages: u64,
    mapping_total_released_pages: u64,
    mapping_peak_active_pages: u64,
    mapping_peak_active_records: u64,
    mapping_file_private_map_count: u64,
    sleep_wake_tick: Option<u64>,
    waitpid_request: Option<UserWaitpidRequest>,
    waitpid_completion: Option<UserWaitpidCompletion>,
    read_request: Option<UserReadRequest>,
    syscall_frame_recorded: bool,
    interrupt_frame_recorded: bool,
    runtime_trap_frame_record_count: u64,
    restored_user_trap_frame_bytes: usize,
    runtime_trap_frame_restore_count: u64,
    last_preemption_reason: UserPreemptionReasonDiagnostics,
    last_resume_path: UserResumePathDiagnostics,
    resume_handoff_count: u64,
    last_resume_address_space_root: Option<PhysicalFrameStart>,
    last_resume_kernel_stack_top: Option<VirtAddr>,
    address_space_reclaiming: bool,
}

impl UserTaskRuntime {
    fn new(
        address_space: UserAddressSpace,
        entry_context: UserTaskContext,
        heap_start: UserVirtualAddress,
        spawn_origin_path: &str,
    ) -> Self {
        Self {
            address_space: Some(address_space),
            saved_frame: entry_context.to_trap_frame(),
            image: UserImageRuntime::new(spawn_origin_path),
            heap: UserHeap::new(heap_start),
            mappings: UserMappings::new(),
            mapping_total_mapped_pages: 0,
            mapping_total_released_pages: 0,
            mapping_peak_active_pages: 0,
            mapping_peak_active_records: 0,
            mapping_file_private_map_count: 0,
            sleep_wake_tick: None,
            waitpid_request: None,
            waitpid_completion: None,
            read_request: None,
            syscall_frame_recorded: false,
            interrupt_frame_recorded: false,
            runtime_trap_frame_record_count: 0,
            restored_user_trap_frame_bytes: 0,
            runtime_trap_frame_restore_count: 0,
            last_preemption_reason: UserPreemptionReasonDiagnostics::None,
            last_resume_path: UserResumePathDiagnostics::None,
            resume_handoff_count: 0,
            last_resume_address_space_root: None,
            last_resume_kernel_stack_top: None,
            address_space_reclaiming: false,
        }
    }

    fn has_schedulable_address_space(&self) -> bool {
        self.address_space.is_some() && !self.address_space_reclaiming
    }

    fn record_user_trap_frame_restore(&mut self) {
        self.restored_user_trap_frame_bytes = core::mem::size_of::<UserTrapFrame>();
        if self.has_recorded_runtime_trap_frame() {
            self.runtime_trap_frame_restore_count =
                self.runtime_trap_frame_restore_count.saturating_add(1);
        }
    }

    fn has_recorded_runtime_trap_frame(&self) -> bool {
        self.syscall_frame_recorded || self.interrupt_frame_recorded
    }

    fn record_resume_handoff(
        &mut self,
        address_space: UserAddressSpace,
        kernel_stack_top: VirtAddr,
    ) {
        self.last_resume_address_space_root = Some(address_space.level_4_frame());
        self.last_resume_kernel_stack_top = Some(kernel_stack_top);
        self.resume_handoff_count = self.resume_handoff_count.saturating_add(1);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UserWaitpidRequest {
    child_task_id: Option<u64>,
    status_buffer: Option<UserWritableRange>,
}

impl UserWaitpidRequest {
    const fn new(child_task_id: Option<u64>, status_buffer: Option<UserWritableRange>) -> Self {
        Self {
            child_task_id,
            status_buffer,
        }
    }

    const fn matches_child(self, child_task_id: u64) -> bool {
        match self.child_task_id {
            Some(requested_child_task_id) => requested_child_task_id == child_task_id,
            None => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct UserWaitpidCompletion {
    child_task_id: u64,
    status_buffer: Option<UserWritableRange>,
    wait_status: u32,
}

impl UserWaitpidCompletion {
    const fn new(
        child_task_id: u64,
        status_buffer: Option<UserWritableRange>,
        wait_status: u32,
    ) -> Self {
        Self {
            child_task_id,
            status_buffer,
            wait_status,
        }
    }
}

#[derive(Debug)]
struct UserImageRuntime {
    generation: u64,
    origin_path_len: usize,
    origin_path_bytes: [u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES],
    path_len: usize,
    path_bytes: [u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES],
    last_execve_state: UserExecveReplacementStateDiagnostics,
    last_execve_old_user_pages: u64,
    last_execve_old_page_table_pages: u64,
}

impl UserImageRuntime {
    fn new(spawn_origin_path: &str) -> Self {
        let mut path_bytes = [0; USER_IMAGE_PATH_DIAGNOSTIC_BYTES];
        let path_len = copy_path_to_diagnostic(spawn_origin_path, &mut path_bytes);
        let mut origin_path_bytes = [0; USER_IMAGE_PATH_DIAGNOSTIC_BYTES];
        let origin_path_len = copy_path_to_diagnostic(spawn_origin_path, &mut origin_path_bytes);
        Self {
            generation: 0,
            origin_path_len,
            origin_path_bytes,
            path_len,
            path_bytes,
            last_execve_state: UserExecveReplacementStateDiagnostics::None,
            last_execve_old_user_pages: 0,
            last_execve_old_page_table_pages: 0,
        }
    }

    fn replace_with_path(&mut self, path: &str) {
        self.generation = self.generation.saturating_add(1);
        self.path_bytes.fill(0);
        self.path_len = copy_path_to_diagnostic(path, &mut self.path_bytes);
        self.last_execve_state = UserExecveReplacementStateDiagnostics::Published;
        self.last_execve_old_user_pages = 0;
        self.last_execve_old_page_table_pages = 0;
    }

    fn record_candidate_drop(&mut self) {
        self.last_execve_state = UserExecveReplacementStateDiagnostics::CandidateDropped;
    }

    const fn snapshot(&self) -> UserImageDiagnosticsSnapshot {
        UserImageDiagnosticsSnapshot::new(
            self.generation,
            self.origin_path_len,
            self.origin_path_bytes,
            self.path_len,
            self.path_bytes,
            UserExecveDiagnosticsSnapshot::new(
                self.last_execve_state,
                self.last_execve_old_user_pages,
                self.last_execve_old_page_table_pages,
            ),
        )
    }

    fn record_last_execve_reclaim(&mut self, reclaim: UserAddressSpaceReclaim) {
        self.last_execve_old_user_pages = reclaim.user_pages();
        self.last_execve_old_page_table_pages = reclaim.page_table_pages();
    }
}

fn copy_path_to_diagnostic(
    path: &str,
    destination: &mut [u8; USER_IMAGE_PATH_DIAGNOSTIC_BYTES],
) -> usize {
    let mut path_len = path.len().min(USER_IMAGE_PATH_DIAGNOSTIC_BYTES);
    while !path.is_char_boundary(path_len) {
        path_len = path_len.saturating_sub(1);
    }
    destination[..path_len].copy_from_slice(&path.as_bytes()[..path_len]);
    path_len
}

/// Source of a captured runtime user trap frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserTrapFrameSource {
    /// Trap frame captured by the SYSCALL return path.
    Syscall,
    /// Trap frame captured by the timer interrupt path.
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
        next_user_kernel_stack_top: Option<VirtAddr>,
        next_user_address_space: Option<UserAddressSpace>,
    },
    EnterUser {
        current_context: *mut u64,
        task_id: u64,
        trap_frame: UserTrapFrame,
        kernel_stack_top: VirtAddr,
        address_space: UserAddressSpace,
    },
}

/// Prepared one-shot user task entry state.
pub(super) struct OneShotUserTask {
    pub(super) trap_frame: UserTrapFrame,
    pub(super) kernel_stack_top: VirtAddr,
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
        metadata: TaskMetadata,
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
            metadata,
            state: TaskState::Ready,
            kind: TaskKind::Kernel,
            context,
            kernel_stack: Some(kernel_stack),
        }
    }

    fn user(
        metadata: TaskMetadata,
        request: UserTaskSpawnRequest<'_>,
        user_context: UserTaskContext,
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
            metadata,
            state: TaskState::Ready,
            kind: TaskKind::User(Box::new(UserTaskRuntime::new(
                request.address_space,
                user_context,
                request.heap_start,
                request.spawn_origin_path,
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

    fn kernel_stack_top(&self) -> Option<VirtAddr> {
        self.kernel_stack.as_ref().map(KernelStack::virtual_top)
    }

    fn kernel_stack_guard_page_virtual_start(&self) -> Option<VirtAddr> {
        self.kernel_stack
            .as_ref()
            .map(KernelStack::guard_page_virtual_start)
    }

    fn kernel_stack_writable_virtual_start(&self) -> Option<VirtAddr> {
        self.kernel_stack
            .as_ref()
            .map(KernelStack::writable_virtual_start)
    }

    fn kernel_stack_virtual_top(&self) -> Option<VirtAddr> {
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

    fn kernel_stack_guard_fault(&self, fault_address: VirtAddr) -> Option<KernelStackGuardFault> {
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

    fn contains_kernel_stack_writable_range(&self, start_address: VirtAddr, byte_len: u64) -> bool {
        self.kernel_stack.as_ref().is_some_and(|kernel_stack| {
            kernel_stack.contains_writable_range(start_address, byte_len)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChildExitRecord {
    parent_task_id: u64,
    child_task_id: u64,
    exit_code: u64,
    collected: bool,
}

impl ChildExitRecord {
    const fn new(parent_task_id: u64, child_task_id: u64, exit_code: u64) -> Self {
        Self {
            parent_task_id,
            child_task_id,
            exit_code,
            collected: false,
        }
    }

    const fn waitable_for_parent(self, parent_task_id: u64, child_task_id: Option<u64>) -> bool {
        if self.parent_task_id != parent_task_id || self.collected {
            return false;
        }

        match child_task_id {
            Some(child_task_id) => self.child_task_id == child_task_id,
            None => true,
        }
    }

    fn mark_collected(&mut self) {
        self.collected = true;
    }

    fn reparent_to_initial_process(&mut self, old_parent_task_id: u64) -> bool {
        if self.parent_task_id != old_parent_task_id || self.collected {
            return false;
        }

        self.parent_task_id = TaskIdentifier::BOOTSTRAP.as_u64();
        true
    }
}

pub(super) struct Scheduler {
    tasks: Vec<Task>,
    current_index: usize,
    next_task_identifier: TaskIdentifier,
    kernel_stack_range_allocator: KernelVirtualRangeAllocator,
    active_user_task_identifiers: Vec<u64>,
    finished_user_exits: VecDeque<UserTaskExit>,
    child_exit_records: Vec<ChildExitRecord>,
    preemption_switch_logged: bool,
    user_resume_logged: bool,
    context_switch_count: u64,
    timer_preemption_count: u64,
    user_entry_count: u64,
    one_shot_user_entry_count: u64,
    timer_user_entry_count: u64,
    timer_user_entry_from_preempted_user_count: u64,
    user_resume_count: u64,
    user_sleep_block_count: u64,
    user_sleep_wake_count: u64,
    user_waitpid_block_count: u64,
    user_waitpid_wake_count: u64,
    user_read_block_count: u64,
    user_read_wake_count: u64,
    finished_task_count: u64,
    reclaimed_user_resource_record_count: u64,
    reclaimed_user_address_space_count: u64,
    reclaimed_user_page_count: u64,
    reclaimed_user_page_table_page_count: u64,
    reclaimed_user_kernel_stack_count: u64,
    reclaimed_user_kernel_stack_writable_pages: u64,
    reclaimed_user_kernel_stack_virtual_pages: u64,
    address_space_reclaim_guard_check_count: u64,
    transition_invariant_check_count: u64,
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
            child_exit_records: Vec::new(),
            preemption_switch_logged: false,
            user_resume_logged: false,
            context_switch_count: 0,
            timer_preemption_count: 0,
            user_entry_count: 0,
            one_shot_user_entry_count: 0,
            timer_user_entry_count: 0,
            timer_user_entry_from_preempted_user_count: 0,
            user_resume_count: 0,
            user_sleep_block_count: 0,
            user_sleep_wake_count: 0,
            user_waitpid_block_count: 0,
            user_waitpid_wake_count: 0,
            user_read_block_count: 0,
            user_read_wake_count: 0,
            finished_task_count: 0,
            reclaimed_user_resource_record_count: 0,
            reclaimed_user_address_space_count: 0,
            reclaimed_user_page_count: 0,
            reclaimed_user_page_table_page_count: 0,
            reclaimed_user_kernel_stack_count: 0,
            reclaimed_user_kernel_stack_writable_pages: 0,
            reclaimed_user_kernel_stack_virtual_pages: 0,
            address_space_reclaim_guard_check_count: 0,
            transition_invariant_check_count: 0,
        }
    }

    fn spawn(&mut self, frame_allocator: &mut PhysicalFrameAllocator, entry: TaskEntry) -> u64 {
        let task_identifier = self.next_task_identifier.allocate();
        let parent_identifier = self.tasks[self.current_index].metadata.get_identifier();
        let parent_current_working_directory = self.tasks[self.current_index]
            .metadata
            .current_working_directory()
            .to_string();
        let child_file_descriptors = self.tasks[self.current_index]
            .metadata
            .file_descriptors()
            .inherit_for_spawn();
        let metadata = TaskMetadata::child(
            task_identifier,
            parent_identifier,
            &parent_current_working_directory,
            child_file_descriptors,
        );
        let task = Task::kernel(
            metadata,
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
            kernel_stack_guard_page_virtual_start.as_u64(),
            kernel_stack_writable_virtual_start.as_u64(),
            kernel_stack_virtual_top.as_u64(),
            kernel_stack_reserved_pages,
            kernel_stack_writable_pages
        );
        task_identifier.as_u64()
    }

    fn spawn_user_task(
        &mut self,
        frame_allocator: &mut PhysicalFrameAllocator,
        request: UserTaskSpawnRequest<'_>,
    ) -> u64 {
        let task_identifier = self.next_task_identifier.allocate();
        let parent_identifier = self.tasks[self.current_index].metadata.get_identifier();
        let parent_current_working_directory = self.tasks[self.current_index]
            .metadata
            .current_working_directory()
            .to_string();
        let child_file_descriptors = self.tasks[self.current_index]
            .metadata
            .file_descriptors()
            .inherit_for_spawn();
        let metadata = TaskMetadata::child(
            task_identifier,
            parent_identifier,
            &parent_current_working_directory,
            child_file_descriptors,
        );
        // SAFETY: The caller provides a mapped user entry point and user stack.
        let user_context = unsafe {
            UserTaskContext::new(
                request.entry_point,
                request.user_stack_top,
                request.entry_arguments,
            )
        };
        let task = Task::user(
            metadata,
            request,
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
        debug_assert_eq!(
            task.metadata.current_working_directory(),
            parent_current_working_directory
        );
        self.tasks.push(task);
        crate::log_info!(
            "task",
            "User task kernel stack prepared: task={} address_space={:#x} heap_start={:#x} bytes={} guard_virtual={:#x} writable_virtual={:#x} virtual_top={:#x} reserved_pages={} writable_pages={} guard_unmapped=true writable_mapped=true",
            task_identifier.as_u64(),
            request.address_space.level_4_frame().as_u64(),
            request.heap_start.as_u64(),
            kernel_stack_bytes,
            kernel_stack_guard_page_virtual_start.as_u64(),
            kernel_stack_writable_virtual_start.as_u64(),
            kernel_stack_virtual_top.as_u64(),
            kernel_stack_reserved_pages,
            kernel_stack_writable_pages
        );
        crate::log_info!(
            "task",
            "User task current directory inherited: parent={} child={} cwd={}",
            parent_identifier.as_u64(),
            task_identifier.as_u64(),
            parent_current_working_directory
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

    fn get_current_working_directory(&self) -> &str {
        self.tasks[self.current_index]
            .metadata
            .current_working_directory()
    }

    fn set_current_working_directory(&mut self, path: alloc::string::String) {
        self.tasks[self.current_index]
            .metadata
            .set_current_working_directory(path);
    }

    fn replace_current_file_descriptor_table(&mut self, file_descriptors: FileDescriptorTable) {
        self.tasks[self.current_index]
            .metadata
            .replace_file_descriptors(file_descriptors);
    }

    fn with_current_file_descriptor_table<R>(
        &mut self,
        operation: impl FnOnce(&mut FileDescriptorTable) -> R,
    ) -> R {
        operation(
            self.tasks[self.current_index]
                .metadata
                .file_descriptors_mut(),
        )
    }

    fn clone_current_file_descriptor_table(&self) -> FileDescriptorTable {
        self.tasks[self.current_index]
            .metadata
            .file_descriptors()
            .clone()
    }

    fn close_current_file_descriptors_on_exec(&mut self) -> usize {
        self.tasks[self.current_index]
            .metadata
            .file_descriptors_mut()
            .close_on_exec_descriptors()
    }

    fn get_current_spawn_descriptor_inheritance_snapshot(
        &self,
    ) -> SpawnDescriptorInheritanceSnapshot {
        self.tasks[self.current_index]
            .metadata
            .file_descriptors()
            .get_spawn_descriptor_inheritance_snapshot()
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

pub(in crate::kernel::task) use facade::complete_pending_user_waitpid_status;
pub(super) use facade::install_user_task_kernel_stack;
pub use facade::{
    activate_user_task, block_current_user_after_syscall, clone_current_file_descriptor_table,
    close_current_file_descriptors_on_exec, close_user_return_preemption_window,
    collect_waitable_child_exit, complete_current_user_read, current_user_task_has_child,
    finish_current_task, get_current_parent_task_id,
    get_current_spawn_descriptor_inheritance_snapshot, get_current_task_id,
    get_current_user_address_space, get_current_working_directory, get_kernel_stack_guard_fault,
    get_kernel_stack_guard_fault_diagnostic_sample, get_scheduler_diagnostics,
    get_scheduler_task_snapshots, has_active_user_tasks, initialize, is_user_task_blocked_for_read,
    prepare_current_user_read, prepare_current_user_sleep, prepare_current_user_waitpid,
    process_current_user_break, process_current_user_mapping, process_current_user_unmapping,
    process_timer_tick, record_current_user_execve_candidate_drop,
    record_current_user_execve_reclaim, record_current_user_trap_frame,
    replace_current_file_descriptor_table, replace_current_user_image,
    run_active_user_tasks_until_empty, run_next_user_task_once, run_user_task_once,
    run_user_task_until_read_block, set_current_working_directory, set_preemption_enabled, spawn,
    spawn_user_task, take_current_user_read_request, verify_scheduler_transition_invariants,
    wake_keyboard_readers, with_current_file_descriptor_table,
};
