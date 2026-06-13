//! `syscalls` kernel console command.

use super::output::{CommandEffect, CommandError, CommandOutput};
use alloc::format;
use alloc::string::{String, ToString};

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if arguments.is_empty() {
        let mut output = CommandOutput::new();
        output.push(
            "syscalls: read write open close fstat lseek mmap munmap brk nanosleep getdents64 exit exit_group openat getpid"
                .to_string(),
        );
        output.push(format_trace_status());
        return Ok(CommandEffect::Output(output));
    }

    let mut parts = arguments.split_whitespace();
    let Some("trace") = parts.next() else {
        return Err(CommandError::UnknownCommand);
    };
    match parts.next() {
        None | Some("status") => {
            if parts.next().is_some() {
                return Err(CommandError::UnknownCommand);
            }
        }
        Some("on") => {
            if parts.next().is_some() {
                return Err(CommandError::UnknownCommand);
            }
            crate::kernel::syscall::set_trace_enabled(true);
        }
        Some("off") => {
            if parts.next().is_some() {
                return Err(CommandError::UnknownCommand);
            }
            crate::kernel::syscall::set_trace_enabled(false);
        }
        Some("reset") => {
            if parts.next().is_some() {
                return Err(CommandError::UnknownCommand);
            }
            crate::kernel::syscall::reset_trace();
        }
        Some(_) => return Err(CommandError::UnknownCommand),
    }

    Ok(CommandEffect::Output(CommandOutput::single(
        format_trace_status(),
    )))
}

fn format_trace_status() -> String {
    let diagnostics = crate::kernel::syscall::get_trace_diagnostics();
    format!(
        "trace: enabled={} records={} last_task={} last_number={} last_result={}",
        diagnostics.enabled(),
        diagnostics.record_count(),
        optional_decimal(diagnostics.last_task_id()),
        optional_decimal(diagnostics.last_syscall_number()),
        optional_hex(diagnostics.last_result())
    )
}

fn optional_decimal(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_string(), |value| value.to_string())
}

fn optional_hex(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_string(), |value| format!("{value:#x}"))
}
