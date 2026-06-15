//! # `kernel::driver::input::keyboard`
//!
//! ## Owns
//! - PS/2 keyboard scancode queue from interrupt handler to main loop
//! - Keyboard scancode decoding and console input dispatch
//! - Decoded keyboard stdin byte queue for userland reads
//!
//! ## Does NOT own
//! - Interrupt routing (-> `arch::x86_64::interrupt_descriptor_table`)
//! - Console state or command execution (-> `kernel::console`)
//!
//! ## Public API
//! - [`push_scancode`] - Called from interrupt handler only
//! - [`process_input`] - Called from main loop only
//! - [`push_stdin_bytes`] - Seed keyboard stdin bytes for smoke setup
//! - [`clear_stdin_buffer`] - Clear queued keyboard stdin bytes for smoke setup
//! - [`get_stdin_bytes`] - Drain decoded keyboard stdin bytes

mod console;
mod decoder;
mod queue;
mod stdin;

/// Push one raw PS/2 keyboard scancode from the interrupt path.
pub fn push_scancode(scancode: u8) {
    queue::push_scancode(scancode);
}

/// Decode and process pending keyboard input.
pub fn process_input() {
    decoder::process_input();
}

/// Push bytes into keyboard-backed standard input.
pub fn push_stdin_bytes(bytes: &[u8]) {
    stdin::push_bytes(bytes);
}

/// Clear queued keyboard-backed standard input bytes.
pub fn clear_stdin_buffer() {
    stdin::clear_buffer();
}

/// Drain queued keyboard-backed standard input bytes into `buffer`.
pub fn get_stdin_bytes(buffer: &mut [u8]) -> usize {
    stdin::get_bytes(buffer)
}
