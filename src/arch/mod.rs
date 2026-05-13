//! # arch
//!
//! ## Owns
//! - Architecture-specific initialization entry points
//!
//! ## Does NOT own
//! - Kernel driver and memory implementations (-> kernel)
//!
//! ## Public API
//! - [`x86_64`] - x86_64 architecture support

pub mod x86_64;
pub use x86_64::*;
