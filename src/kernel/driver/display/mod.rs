//! # kernel::driver::display
//!
//! ## Owns
//! - Framebuffer-backed graphics output
//! - Text rendering and drawing primitives
//!
//! ## Does NOT own
//! - Keyboard or mouse packet processing (-> kernel::driver::input)
//!
//! ## Public API
//! - [`framebuffer`] - Framebuffer graphics driver

pub mod framebuffer;
