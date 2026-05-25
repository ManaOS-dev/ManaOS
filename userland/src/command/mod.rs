//! # `mana_userland::command`
//!
//! ## Owns
//! - Fixed-buffer userland command dispatch
//! - Single-pipe command execution
//!
//! ## Does NOT own
//! - Kernel console command dispatch
//! - Process creation or `execve`
//! - Heap allocation
//!
//! ## Public API
//! - [`run_line`] - Execute one command line with optional `left | right` pipeline

mod cat;
mod clear;
mod dispatch;
mod echo;
mod grep;
mod output;
mod pipeline;

pub use output::{CommandError, CommandOutput, COMMAND_BUFFER_BYTES};
pub use pipeline::run_line;
