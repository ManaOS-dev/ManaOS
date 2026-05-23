//! # `kernel::sync`
//!
//! ## Owns
//! - Kernel synchronization primitives
//!
//! ## Does NOT own
//! - Device driver state
//! - Scheduler policy
//!
//! ## Public API
//! - [`ring_buffer`] - Single-producer single-consumer queue

pub mod ring_buffer;
