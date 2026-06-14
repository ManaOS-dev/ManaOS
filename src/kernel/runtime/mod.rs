//! # `kernel::runtime`
//!
//! ## Owns
//! - Runtime module composition
//! - Main-loop service re-exports
//!
//! ## Does NOT own
//! - Keyboard or mouse input queues
//! - Display command internals
//! - Timer hardware configuration
//! - Runtime loop state and tick processing (-> `service`)
//!
//! ## Public API
//! - [`initialize`] - Initialize runtime counters
//! - [`get_fps`] - Return the last calculated frames-per-second value
//! - [`tick`] - Run one iteration of the main loop

mod service;

pub use service::{get_fps, initialize, tick};
