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
//! - [`console`] - Kernel command console
//! - [`diagnostic`] - Kernel diagnostic logging
//! - [`driver`] - Kernel device drivers
//! - [`filesystem`] - Kernel virtual filesystem and file descriptors
//! - [`interrupt`] - Kernel-side interrupt event routing
//! - [`logger`] - Boot phase logging
//! - [`memory`] - Kernel memory management
//! - [`profiler`] - Lightweight profiling support
//! - [`serial`] - Serial output
//! - [`sync`] - Synchronization primitives
//! - [`syscall`] - Kernel syscall dispatch
//! - [`task`] - Task context support
//! - [`time`] - Kernel time source boundary

pub mod boot;
pub mod console;
pub mod diagnostic;
pub mod driver;
pub mod filesystem;
pub mod interrupt;
pub mod logger;
pub mod memory;
pub mod profiler;
pub mod runtime;
pub mod serial;
pub mod sync;
pub mod syscall;
pub mod task;
pub mod time;
