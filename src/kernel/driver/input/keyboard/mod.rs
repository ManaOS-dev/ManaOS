//! # `kernel::driver::input::keyboard`
//!
//! ## Owns
//! - PS/2 keyboard scancode queue from interrupt handler to main loop
//! - Keyboard scancode decoding and console input dispatch
//!
//! ## Does NOT own
//! - Interrupt routing (-> `arch::x86_64::interrupt_descriptor_table`)
//! - Console state or command execution (-> `kernel::console`)
//!
//! ## Public API
//! - [`push_scancode`] - Called from interrupt handler only
//! - [`process_input`] - Called from main loop only

mod console;
mod decoder;
mod queue;

/// Push one raw PS/2 keyboard scancode from the interrupt path.
pub fn push_scancode(scancode: u8) {
    queue::push_scancode(scancode);
}

/// Decode and process pending keyboard input.
pub fn process_input() {
    decoder::process_input();
}
