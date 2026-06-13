//! Active user runtime frame allocator access for syscall-time mappings.

use spin::Mutex;

use super::frame_allocator::PhysicalFrameAllocator;

static USER_RUNTIME_FRAME_ALLOCATOR: Mutex<Option<usize>> = Mutex::new(None);

/// Register the physical frame allocator used while user tasks can issue syscalls.
pub fn register_user_runtime_frame_allocator(frame_allocator: &mut PhysicalFrameAllocator) {
    let pointer = core::ptr::from_mut(frame_allocator).addr();
    let mut runtime_allocator = USER_RUNTIME_FRAME_ALLOCATOR.lock();
    assert!(
        runtime_allocator.is_none(),
        "user runtime frame allocator must not already be registered"
    );
    *runtime_allocator = Some(pointer);
}

/// Clear the active user runtime frame allocator.
pub fn clear_user_runtime_frame_allocator() {
    let mut runtime_allocator = USER_RUNTIME_FRAME_ALLOCATOR.lock();
    assert!(
        runtime_allocator.take().is_some(),
        "user runtime frame allocator must be registered before clearing"
    );
}

/// Run `process` with the active user runtime frame allocator.
///
/// Returns `None` when no user runtime allocator is registered.
pub fn with_user_runtime_frame_allocator<T>(
    process: impl FnOnce(&mut PhysicalFrameAllocator) -> T,
) -> Option<T> {
    let runtime_allocator = USER_RUNTIME_FRAME_ALLOCATOR.lock();
    let pointer = runtime_allocator.as_ref().copied()?;
    let frame_allocator = pointer as *mut PhysicalFrameAllocator;
    // SAFETY: The boot composition root registers the frame allocator only
    // while one-shot user tasks may issue syscalls. ManaOS runs this path on
    // one CPU, and this mutex serializes syscall-time allocator access.
    Some(process(unsafe { &mut *frame_allocator }))
}
