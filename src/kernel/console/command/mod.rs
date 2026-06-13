//! # `kernel::console::command`
//!
//! ## Owns
//! - Kernel console command parsing
//! - Single-pipe command execution
//! - Dispatch to command-focused handlers
//!
//! ## Does NOT own
//! - Console rendering and input state (-> `kernel::console`)
//! - Filesystem namespace state (-> `kernel::filesystem`)
//! - Storage controller probing (-> `kernel::driver::storage`)
//!
//! ## Public API
//! - [`execute`] - Parse and run one submitted console command

mod cat;
mod cd;
mod clear;
mod context;
mod dispatch;
mod echo;
mod fps;
mod grep;
mod help;
mod hexdump;
mod ls;
mod memory;
mod mounts;
mod output;
mod pipeline;
mod pwd;
mod stat;
mod storage;
mod syscalls;
mod tasks;
mod ticks;

use alloc::format;

pub(super) fn execute(command: &str) {
    if command.is_empty() {
        return;
    }

    super::push_output(format!("> {command}"));
    let is_pipeline = command.contains('|');
    match pipeline::run_line(command) {
        Ok(output::CommandEffect::Output(output)) => {
            if is_pipeline {
                crate::log_info!(
                    "console",
                    "Pipeline command completed: command=\"{}\" output_lines={}",
                    command,
                    output.lines().len()
                );
            }
            for line in output.lines() {
                super::push_output(line.clone());
            }
        }
        Ok(output::CommandEffect::Clear) => super::clear_output(),
        Err(error) => super::push_output(error.message(command)),
    }
}

pub(super) fn verify_pipeline_smoke(command: &str) -> Option<usize> {
    if !command.contains('|') {
        return None;
    }

    match pipeline::run_line(command).ok()? {
        output::CommandEffect::Output(output) => Some(output.lines().len()),
        output::CommandEffect::Clear => None,
    }
}

pub(super) fn verify_command_smoke(command: &str) -> Option<usize> {
    if command.contains('|') {
        return None;
    }

    match pipeline::run_line(command).ok()? {
        output::CommandEffect::Output(output) => Some(output.lines().len()),
        output::CommandEffect::Clear => None,
    }
}
