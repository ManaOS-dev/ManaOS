//! Kernel task context layout.

use core::mem;

/// A kernel task entry point.
pub type TaskEntry = extern "C" fn() -> !;

const TASK_CONTEXT_STACK_POINTER_OFFSET: usize = 0;
const TASK_CONTEXT_REGISTER_15_OFFSET: usize = 8;
const TASK_CONTEXT_REGISTER_14_OFFSET: usize = 16;
const TASK_CONTEXT_REGISTER_13_OFFSET: usize = 24;
const TASK_CONTEXT_REGISTER_12_OFFSET: usize = 32;
const TASK_CONTEXT_REGISTER_BX_OFFSET: usize = 40;
const TASK_CONTEXT_BASE_POINTER_OFFSET: usize = 48;
const TASK_CONTEXT_FLAGS_OFFSET: usize = 56;
const TASK_CONTEXT_BYTES: usize = 64;

/// Saved callee-saved context for an `x86_64` kernel task.
///
/// The field order is part of the assembly contract with
/// `arch/x86_64/context_switch.s`:
/// - offset 0: `rsp`
/// - offset 8: `r15`
/// - offset 16: `r14`
/// - offset 24: `r13`
/// - offset 32: `r12`
/// - offset 40: `rbx`
/// - offset 48: `rbp`
/// - offset 56: `rflags`
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TaskContext {
    stack_pointer: u64,
    register_15: u64,
    register_14: u64,
    register_13: u64,
    register_12: u64,
    register_bx: u64,
    base_pointer: u64,
    flags: u64,
}

impl TaskContext {
    /// Create an empty task context.
    pub const fn new() -> Self {
        Self {
            stack_pointer: 0,
            register_15: 0,
            register_14: 0,
            register_13: 0,
            register_12: 0,
            register_bx: 0,
            base_pointer: 0,
            flags: 0x202,
        }
    }

    /// Create an initial context that returns into `entry` on first switch.
    ///
    /// # Safety
    ///
    /// `stack_top` must point one byte past a writable kernel stack that remains
    /// owned by the task for the full lifetime of this context.
    pub unsafe fn from_stack(stack_top: usize, entry: TaskEntry) -> Self {
        let aligned_stack_top = stack_top & !0x0f;
        let stack_pointer = aligned_stack_top - 16;
        let entry_slot = stack_pointer as *mut usize;

        // SAFETY: The caller guarantees the stack range is writable and owned by
        // this task. The slot is 16-byte aligned and reserved for the first ret.
        unsafe {
            entry_slot.write(entry as usize);
        }

        Self {
            stack_pointer: stack_pointer as u64,
            ..Self::new()
        }
    }

    /// Return a mutable pointer suitable for the architecture context switch.
    pub fn as_mut_pointer(&mut self) -> *mut u64 {
        core::ptr::addr_of_mut!(self.stack_pointer)
    }

    /// Return an immutable pointer suitable for the architecture context switch.
    pub fn as_pointer(&self) -> *const u64 {
        core::ptr::addr_of!(self.stack_pointer)
    }

    /// Return true when this context has never been initialized with a stack.
    pub fn is_empty(&self) -> bool {
        self.stack_pointer == 0
    }
}

impl Default for TaskContext {
    fn default() -> Self {
        Self::new()
    }
}

const _: () = {
    assert!(mem::size_of::<TaskContext>() == TASK_CONTEXT_BYTES);
    assert!(mem::offset_of!(TaskContext, stack_pointer) == TASK_CONTEXT_STACK_POINTER_OFFSET);
    assert!(mem::offset_of!(TaskContext, register_15) == TASK_CONTEXT_REGISTER_15_OFFSET);
    assert!(mem::offset_of!(TaskContext, register_14) == TASK_CONTEXT_REGISTER_14_OFFSET);
    assert!(mem::offset_of!(TaskContext, register_13) == TASK_CONTEXT_REGISTER_13_OFFSET);
    assert!(mem::offset_of!(TaskContext, register_12) == TASK_CONTEXT_REGISTER_12_OFFSET);
    assert!(mem::offset_of!(TaskContext, register_bx) == TASK_CONTEXT_REGISTER_BX_OFFSET);
    assert!(mem::offset_of!(TaskContext, base_pointer) == TASK_CONTEXT_BASE_POINTER_OFFSET);
    assert!(mem::offset_of!(TaskContext, flags) == TASK_CONTEXT_FLAGS_OFFSET);
};
