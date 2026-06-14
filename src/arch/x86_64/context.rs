//! Assembly-backed task and user-mode context entry points.

core::arch::global_asm!(include_str!("context_switch.s"));
core::arch::global_asm!(include_str!("interrupt_entry.s"));

extern "C" {
    /// Switch from one saved task context to another.
    pub fn context_switch(current_context: *mut u64, next_context: *const u64);
    /// Save a task context and restore a user trap frame.
    pub fn switch_to_user_mode(current_context: *mut u64, context: *const u64);
    /// Restore a user trap frame and return when the user task exits through `SYS_EXIT`.
    pub fn enter_user_mode_returnable(context: *const u64);
}

/// Switch from one saved task context to another.
///
/// # Safety
///
/// `current_context` and `next_context` must point to valid task context storage
/// with the layout expected by `context_switch.s`. The pointed tasks must remain
/// alive across the switch.
#[cfg(target_os = "uefi")]
pub unsafe fn switch_context(current_context: *mut u64, next_context: *const u64) {
    context_switch(current_context, next_context);
}

/// Save a task context and restore a user trap frame.
///
/// # Safety
///
/// `current_context` must point to valid task context storage. `context` must
/// point to a valid user trap frame whose code and stack addresses are mapped
/// as user-accessible pages. This call may return after the saved
/// `current_context` is later scheduled again.
#[cfg(target_os = "uefi")]
pub unsafe fn switch_to_user_mode_context(current_context: *mut u64, context: *const u64) {
    switch_to_user_mode(current_context, context);
}

/// Restore a user trap frame and return when the user task exits through `SYS_EXIT`.
///
/// # Safety
///
/// `context` must point to a valid user trap frame whose code and
/// stack addresses are mapped as user-accessible pages.
#[cfg(target_os = "uefi")]
pub unsafe fn enter_user_mode_once(context: *const u64) {
    enter_user_mode_returnable(context);
}

/// Switch from one saved task context to another.
///
/// # Safety
///
/// This host-build stub is never used by the UEFI kernel runtime.
#[cfg(not(target_os = "uefi"))]
pub unsafe fn switch_context(_current_context: *mut u64, _next_context: *const u64) {}

/// Save a task context and restore a user trap frame.
///
/// # Safety
///
/// This host-build stub is never used by the UEFI kernel runtime.
#[cfg(not(target_os = "uefi"))]
pub unsafe fn switch_to_user_mode_context(_current_context: *mut u64, _context: *const u64) {}

/// Restore a user trap frame and return when the user task exits through `SYS_EXIT`.
///
/// # Safety
///
/// This host-build stub is never used by the UEFI kernel runtime.
#[cfg(not(target_os = "uefi"))]
pub unsafe fn enter_user_mode_once(_context: *const u64) {}
