//! # `kernel::driver`
//!
//! ## Owns
//! - Kernel device driver module boundaries
//!
//! ## Does NOT own
//! - Architecture-specific interrupt setup (-> `arch`)
//! - Device-independent memory management (-> `kernel::memory`)
//!
//! ## Public API
//! - [`display`] - Display and framebuffer drivers
//! - [`input`] - Keyboard and mouse input drivers
//! - [`storage`] - Storage controller drivers

pub mod display;
pub mod input;
pub mod storage;
