//! # shared
//!
//! ## Owns
//! - Cross-boundary records shared by architecture and kernel modules
//!
//! ## Does NOT own
//! - Architecture-specific interrupt entry code (-> `arch`)
//! - Kernel scheduling policy (-> `kernel::task`)
//!
//! ## Public API
//! - [`TIMER_TICKS_PER_SECOND`] - Kernel timer tick frequency
//! - [`TimerInterruptFrame`] - Complete timer interrupt register snapshot

mod timer_interrupt_frame;

pub use timer_interrupt_frame::TimerInterruptFrame;

/// Number of scheduler timer ticks produced each second.
///
/// The `x86_64` boot path programs the PIT to this frequency; kernel time users
/// depend on this value when converting wall-clock durations to ticks.
pub const TIMER_TICKS_PER_SECOND: u64 = 1000;
