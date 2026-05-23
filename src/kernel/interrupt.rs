//! # `kernel::interrupt`
//!
//! ## Owns
//! - Kernel-side interrupt event routing
//!
//! ## Does NOT own
//! - Architecture-specific interrupt descriptor tables (-> `arch`)
//! - Hardware interrupt acknowledgement (-> `arch`)
//! - Input byte queues (-> `kernel::driver::input`)
//!
//! ## Public API
//! - [`process_timer_tick`] - Route timer ticks to the scheduler
//! - [`push_keyboard_byte`] - Route keyboard bytes to the keyboard queue
//! - [`push_mouse_byte`] - Route mouse bytes to the mouse queue

/// Route one timer interrupt tick to the kernel scheduler.
pub fn process_timer_tick() {
    crate::kernel::task::process_timer_tick();
}

/// Route one keyboard byte to the keyboard input queue.
pub fn push_keyboard_byte(byte: u8) {
    crate::kernel::driver::input::keyboard::push_scancode(byte);
}

/// Route one mouse byte to the mouse input queue.
pub fn push_mouse_byte(byte: u8) {
    crate::kernel::driver::input::mouse::push_byte(byte);
}
