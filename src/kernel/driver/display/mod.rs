//! # `kernel::driver::display`
//!
//! ## Owns
//! - Framebuffer-backed graphics output
//! - Text rendering and drawing primitives
//!
//! ## Does NOT own
//! - Keyboard or mouse packet processing (-> `kernel::driver::input`)
//!
//! ## Public API
//! - [`framebuffer`] - Framebuffer graphics driver
//! - [`renderer`] - Display scene rendering helpers

pub mod color;
pub mod command;
pub mod framebuffer;
pub mod renderer;
