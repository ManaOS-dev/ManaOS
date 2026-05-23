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
//! - [`cursor`] - Display-owned cursor renderer
//! - [`font`] - Font types and loaded font assets
//! - [`renderer`] - Display scene rendering helpers

pub mod color;
pub mod command;
pub mod cursor;
pub mod font;
pub mod framebuffer;
pub mod renderer;
