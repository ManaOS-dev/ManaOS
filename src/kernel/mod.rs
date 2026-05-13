//! # kernel
//!
//! ## Owns
//! - Kernel drivers, memory management, synchronization, task support, logging, and profiling
//!
//! ## Does NOT own
//! - Architecture-specific CPU and interrupt setup (-> arch)
//! - Bootloader entry wiring (-> main.rs)
//!
//! ## Public API
//! - [`driver`] - Kernel device drivers
//! - [`logger`] - Boot phase logging
//! - [`memory`] - Kernel memory management
//! - [`profiler`] - Lightweight profiling support
//! - [`serial`] - Serial output
//! - [`sync`] - Synchronization primitives
//! - [`task`] - Task context support

pub mod driver;
pub mod logger;
pub mod memory;
pub mod profiler;
pub mod serial;
pub mod sync;
pub mod task;
