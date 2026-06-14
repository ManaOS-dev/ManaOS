//! # `kernel::diagnostic`
//!
//! ## Owns
//! - Kernel diagnostic output helpers
//! - Structured serial log formatting
//! - Boot-time smoke diagnostic orchestration
//! - Human-readable boot summary output
//!
//! ## Does NOT own
//! - Raw serial port access (-> `kernel::serial`)
//! - Boot-services console logging (-> `kernel::logger`)
//!
//! ## Public API
//! - [`log`] - Structured kernel log output
//! - [`smoke`] - Boot-time kernel smoke diagnostics
//! - [`summary`] - Human-readable boot summary output

pub mod log;
/// Boot-time kernel smoke diagnostics.
pub mod smoke;
/// Human-readable boot summary output.
pub mod summary;
