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
//! - [`TimerInterruptFrame`] - Complete timer interrupt register snapshot

mod timer_interrupt_frame;

pub use timer_interrupt_frame::TimerInterruptFrame;
