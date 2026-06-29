//! Architecture callback boundary for task switching.

use crate::kernel::memory::address::VirtAddr;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Architecture function that switches between two saved kernel contexts.
pub type ContextSwitchFunction = unsafe fn(*mut u64, *const u64);

/// Architecture function that saves a task context and restores a user frame.
pub type UserModeSwitchFunction = unsafe fn(*mut u64, *const u64);
/// Architecture function that restores a user trap frame and returns after `SYS_EXIT`.
pub type ReturnableUserModeEntryFunction = unsafe fn(*const u64);
/// Architecture function that installs the Ring 0 stack for user-mode traps.
pub type KernelStackInstallFunction = fn(VirtAddr);

static CONTEXT_SWITCH_FUNCTION: AtomicUsize = AtomicUsize::new(0);
static USER_MODE_SWITCH_FUNCTION: AtomicUsize = AtomicUsize::new(0);
static RETURNABLE_USER_MODE_ENTRY_FUNCTION: AtomicUsize = AtomicUsize::new(0);
static KERNEL_STACK_INSTALL_FUNCTION: AtomicUsize = AtomicUsize::new(0);

/// Register the architecture context switch entry point.
pub fn register_context_switch(function: ContextSwitchFunction) {
    CONTEXT_SWITCH_FUNCTION.store(function as usize, Ordering::Release);
}

/// Register the architecture user-mode context switch entry point.
pub fn register_user_mode_switch(function: UserModeSwitchFunction) {
    USER_MODE_SWITCH_FUNCTION.store(function as usize, Ordering::Release);
}

/// Register the architecture returnable user trap-frame entry point.
pub fn register_returnable_user_mode_entry(function: ReturnableUserModeEntryFunction) {
    RETURNABLE_USER_MODE_ENTRY_FUNCTION.store(function as usize, Ordering::Release);
}

/// Register the architecture Ring 0 stack installation entry point.
pub fn register_kernel_stack_installer(function: KernelStackInstallFunction) {
    KERNEL_STACK_INSTALL_FUNCTION.store(function as usize, Ordering::Release);
}

/// Switch from one saved task context to another.
///
/// # Safety
///
/// `current_context` and `next_context` must point to valid architecture task
/// context storage. The pointed tasks must remain alive across the switch.
///
/// # Panics
///
/// Panics if the architecture context switch entry point has not been
/// registered by the composition root.
pub unsafe fn switch_context(current_context: *mut u64, next_context: *const u64) {
    let function = CONTEXT_SWITCH_FUNCTION.load(Ordering::Acquire);
    assert!(
        function != 0,
        "architecture context switch function must be registered before scheduling"
    );

    // SAFETY: The stored value came from register_context_switch and zero was
    // handled above as the unregistered state.
    let function: ContextSwitchFunction = unsafe { core::mem::transmute(function) };
    // SAFETY: The caller upholds the context pointer validity contract.
    unsafe {
        function(current_context, next_context);
    }
}

/// Save the current task context and restore a user trap frame.
///
/// # Safety
///
/// `current_context` must point to valid architecture task context storage.
/// `context` must point to a valid user trap frame whose code and stack
/// addresses are mapped as user-accessible pages. This call may return after
/// the saved `current_context` is later scheduled again.
///
/// # Panics
///
/// Panics if the architecture user-mode switch entry point has not been
/// registered by the composition root.
pub unsafe fn switch_to_user_mode(current_context: *mut u64, context: *const u64) {
    let function = USER_MODE_SWITCH_FUNCTION.load(Ordering::Acquire);
    assert!(
        function != 0,
        "architecture user-mode switch function must be registered before scheduling"
    );

    // SAFETY: The stored value came from register_user_mode_switch and zero
    // was handled above as the unregistered state.
    let function: UserModeSwitchFunction = unsafe { core::mem::transmute(function) };
    // SAFETY: The caller upholds the context pointer and user trap-frame
    // pointer validity contracts.
    unsafe {
        function(current_context, context);
    }
}

/// Restore a user trap frame and return after the task exits through `SYS_EXIT`.
///
/// # Safety
///
/// `context` must point to a valid user trap frame whose code and
/// stack addresses are mapped as user-accessible pages.
///
/// # Panics
///
/// Panics if the architecture returnable user-mode entry point has not been
/// registered by the composition root.
pub unsafe fn enter_user_mode_once(context: *const u64) {
    let function = RETURNABLE_USER_MODE_ENTRY_FUNCTION.load(Ordering::Acquire);
    assert!(
        function != 0,
        "architecture returnable user-mode entry function must be registered before running demos"
    );

    // SAFETY: The stored value came from register_returnable_user_mode_entry and
    // zero was handled above as the unregistered state.
    let function: ReturnableUserModeEntryFunction = unsafe { core::mem::transmute(function) };
    // SAFETY: The caller upholds the user trap-frame pointer validity contract.
    unsafe {
        function(context);
    }
}

/// Install the kernel stack used by future user-mode traps.
///
/// # Panics
///
/// Panics if the architecture kernel stack installer has not been registered by
/// the composition root.
pub fn install_kernel_stack(stack_top: VirtAddr) {
    let function = KERNEL_STACK_INSTALL_FUNCTION.load(Ordering::Acquire);
    assert!(
        function != 0,
        "architecture kernel stack installer must be registered before entering user mode"
    );

    // SAFETY: The stored value came from register_kernel_stack_installer and
    // zero was handled above as the unregistered state.
    let function: KernelStackInstallFunction = unsafe { core::mem::transmute(function) };
    function(stack_top);
}
