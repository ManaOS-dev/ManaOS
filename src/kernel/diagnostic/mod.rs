//! # `kernel::diagnostic`
//!
//! ## Owns
//! - Kernel diagnostic output helpers
//! - Structured serial log formatting
//!
//! ## Does NOT own
//! - Raw serial port access (-> `kernel::serial`)
//! - Boot-services console logging (-> `kernel::logger`)
//!
//! ## Public API
//! - [`log`] - Structured kernel log output

pub mod log;
