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
        "preemption: state={} enabled={} switches={} timer_user_preemptions={} user_entries={} user_resumes={} finished={} pending_user_exits={} exit_window_closes={}",
        diagnostics.preemption_state().as_str(),
        diagnostics.preemption_enabled(),
        diagnostics.context_switches(),
        diagnostics.timer_preemptions(),
        diagnostics.user_entries(),
        diagnostics.user_resumes(),
        diagnostics.finished_tasks(),
        diagnostics.pending_user_exits(),
        diagnostics.user_exit_preemption_window_closes()
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
    output.push(format!(
        "user_vm_layout: program_base={:#x} heap_end={:#x} mmap_start={:#x} mmap_end={:#x} stack_start={:#x} stack_slot_bytes={}",
        crate::kernel::memory::user_layout::USER_PROGRAM_BASE,
        crate::kernel::memory::user_layout::USER_HEAP_END,
        crate::kernel::memory::user_layout::USER_MAPPING_BASE,
        crate::kernel::memory::user_layout::USER_MAPPING_END,
        crate::kernel::memory::user_layout::USER_STACK_REGION_BASE,
        crate::kernel::memory::user_layout::USER_STACK_SLOT_BYTES
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
        if let Some(user_virtual_memory) = snapshot.user_virtual_memory() {
            output.push(format!(
                "task_vm: id={} heap_base={:#x} heap_break={:#x} heap_pages={} mmap_next={:#x} mmap_pages={} mmap_records={}",
                snapshot.task_id(),
                user_virtual_memory.heap_base(),
                user_virtual_memory.heap_break(),
                user_virtual_memory.heap_mapped_pages(),
                user_virtual_memory.mapping_next_start(),
                user_virtual_memory.mapping_active_pages(),
                user_virtual_memory.mapping_active_records()
            ));
        }
    }
    Ok(CommandEffect::Output(output))
}
