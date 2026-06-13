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
        "tasks: total={} kernel={} user={} active_user_tasks={} active_user_address_spaces={}",
        diagnostics.total_tasks(),
        diagnostics.kernel_tasks(),
        diagnostics.user_tasks(),
        diagnostics.active_user_tasks(),
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
        "preemption: switches={} timer_user_preemptions={} user_entries={} user_resumes={} finished={} pending_user_exits={}",
        diagnostics.context_switches(),
        diagnostics.timer_preemptions(),
        diagnostics.user_entries(),
        diagnostics.user_resumes(),
        diagnostics.finished_tasks(),
        diagnostics.pending_user_exits()
    ));
    output.push(format!(
        "resources: reclaimed_user_resource_records={} reclaimed_user_kernel_stacks={} reclaimed_kernel_stack_writable_pages={} reclaimed_kernel_stack_virtual_pages={}",
        diagnostics.reclaimed_user_resource_records(),
        diagnostics.reclaimed_user_kernel_stacks(),
        diagnostics.reclaimed_user_kernel_stack_writable_pages(),
        diagnostics.reclaimed_user_kernel_stack_virtual_pages()
    ));
    output.push(format!(
        "user_exit_return: stack_sets={} stack_takes={}",
        diagnostics.user_exit_return_stack_sets(),
        diagnostics.user_exit_return_stack_takes()
    ));
    let Some(snapshots) = crate::kernel::task::get_scheduler_task_snapshots() else {
        output.push("task_table: unavailable".to_string());
        return Ok(CommandEffect::Output(output));
    };
    for snapshot in snapshots {
        output.push(format!(
            "task: id={} parent={} kind={} state={} active={} address_space_owned={} kernel_stack_owned={}",
            snapshot.task_id(),
            snapshot
                .parent_task_id()
                .map_or_else(|| "-".to_string(), |task_id| task_id.to_string()),
            snapshot.kind().as_str(),
            snapshot.state().as_str(),
            snapshot.active(),
            snapshot.address_space_owned(),
            snapshot.kernel_stack_owned()
        ));
    }
    Ok(CommandEffect::Output(output))
}
