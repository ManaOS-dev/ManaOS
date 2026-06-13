//! `tasks` kernel console command.

use super::output::{CommandEffect, CommandError, CommandOutput};
use alloc::format;
use alloc::string::ToString;

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if !arguments.is_empty() {
        return Err(CommandError::UnknownCommand);
    }

    let Some(diagnostics) = crate::kernel::task::get_scheduler_diagnostics() else {
        return Ok(CommandEffect::Output(CommandOutput::single(
            "tasks: scheduler unavailable".to_string(),
        )));
    };
    let states = diagnostics.states();

    let mut output = CommandOutput::new();
    output.push(format!(
        "tasks: total={} kernel={} user={} active_user_address_spaces={}",
        diagnostics.total_tasks(),
        diagnostics.kernel_tasks(),
        diagnostics.user_tasks(),
        diagnostics.active_user_address_spaces()
    ));
    output.push(format!(
        "states: ready={} running={} blocked={} finished={}",
        states.ready(),
        states.running(),
        states.blocked(),
        states.finished()
    ));
    output.push(format!(
        "preemption: switches={} timer_user_preemptions={} user_entries={} user_resumes={} finished={}",
        diagnostics.context_switches(),
        diagnostics.timer_preemptions(),
        diagnostics.user_entries(),
        diagnostics.user_resumes(),
        diagnostics.finished_tasks()
    ));
    Ok(CommandEffect::Output(output))
}
