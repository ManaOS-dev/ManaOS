//! # kernel::driver::input
//!
//! ## Owns
//! - Input device module boundaries
//!
//! ## Does NOT own
//! - Cursor rendering primitives (-> kernel::driver::display)
//!
//! ## Public API
//! - [`keyboard`] - PS/2 keyboard input
//! - [`mouse`] - PS/2 mouse input

pub mod keyboard;
pub mod mouse;
