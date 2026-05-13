//! Task context storage.

use core::mem;

/// A kernel task entry point.
pub type TaskEntry = extern "C" fn() -> !;

/// Saved callee-saved context for an `x86_64` kernel task.
///
/// The field order is part of the assembly contract with
/// `arch/x86_64/context_switch.s`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TaskContext {
    stack_pointer: u64,
    register_15: u64,
    register_14: u64,
    register_13: u64,
    register_12: u64,
    register_base: u64,
    base_pointer: u64,
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
            register_base: 0,
            base_pointer: 0,
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
}

impl Default for TaskContext {
    fn default() -> Self {
        Self::new()
    }
}

const _: () = assert!(mem::size_of::<TaskContext>() == 56);
