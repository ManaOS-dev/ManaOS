//! Kernel task stack owner metadata.

use alloc::boxed::Box;
use alloc::vec;

const DEFAULT_KERNEL_STACK_SIZE: usize = 16 * 1024;

/// Heap-backed kernel stack owned by one schedulable task.
///
/// This is the transitional metadata shape before guarded kernel stack
/// mappings exist. The buffer keeps the stack memory alive for the lifetime of
/// the task context, and the explicit top/base accessors define the future
/// replacement boundary for guarded mapped stacks.
pub(super) struct KernelStack {
    buffer: Box<[u8]>,
}

impl KernelStack {
    /// Allocate the current default heap-backed kernel stack.
    pub(super) fn new_default() -> Self {
        Self {
            buffer: vec![0; DEFAULT_KERNEL_STACK_SIZE].into_boxed_slice(),
        }
    }

    /// Return the lowest writable address in this stack buffer.
    pub(super) fn base(&self) -> usize {
        self.buffer.as_ptr() as usize
    }

    /// Return one byte past the highest writable address in this stack buffer.
    pub(super) fn top(&self) -> usize {
        self.buffer.as_ptr() as usize + self.buffer.len()
    }

    /// Return the writable stack size in bytes.
    pub(super) fn byte_len(&self) -> usize {
        self.buffer.len()
    }
}
