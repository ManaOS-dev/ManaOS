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
//! - [`PageFaultReport`] - Page-fault diagnostic record
//! - [`TimerInterruptFrame`] - Complete timer interrupt register snapshot
//! - [`verify_typed_page_fault_report`] - Page-fault report wrapper self-check
//! - [`verify_typed_timer_interrupt_frame`] - Timer frame wrapper self-check

mod page_fault_report;
mod timer_interrupt_frame;

pub use page_fault_report::{
    verify_typed_page_fault_report, PageFaultAddress, PageFaultErrorBits,
    PageFaultInstructionPointer, PageFaultReport,
};
pub use timer_interrupt_frame::{
    verify_typed_timer_interrupt_frame, TimerFrameInstructionPointer, TimerFrameStackPointer,
    TimerFrameStorageAddress, TimerInterruptFrame,
};

/// Number of scheduler timer ticks produced each second.
///
/// The `x86_64` boot path programs the PIT to this frequency; kernel time users
/// depend on this value when converting wall-clock durations to ticks.
pub const TIMER_TICKS_PER_SECOND: u64 = 1000;
