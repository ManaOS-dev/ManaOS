//! `tasks` kernel console command.

use super::output::{CommandEffect, CommandError, CommandOutput};
use alloc::format;
use alloc::string::ToString;

use crate::kernel::task::{
    SchedulerTaskSnapshot, UserImageDiagnosticsSnapshot, UserVirtualMemorySnapshot,
};

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
        "preemption: state={} enabled={} switches={} timer_user_preemptions={} user_entries={} one_shot_user_entries={} timer_user_entries={} user_resumes={} user_sleep_blocks={} user_sleep_wakes={} user_waitpid_blocks={} user_waitpid_wakes={} finished={} pending_user_exits={} return_window_closes={}",
        diagnostics.preemption_state().as_str(),
        diagnostics.preemption_enabled(),
        diagnostics.context_switches(),
        diagnostics.timer_preemptions(),
        diagnostics.user_entries(),
        diagnostics.one_shot_user_entries(),
        diagnostics.timer_user_entries(),
        diagnostics.user_resumes(),
        diagnostics.user_sleep_blocks(),
        diagnostics.user_sleep_wakes(),
        diagnostics.user_waitpid_blocks(),
        diagnostics.user_waitpid_wakes(),
        diagnostics.finished_tasks(),
        diagnostics.pending_user_exits(),
        diagnostics.user_return_preemption_window_closes()
    ));
    output.push(format!(
        "resources: reclaimed_user_resource_records={} reclaimed_user_address_spaces={} reclaimed_user_pages={} reclaimed_user_page_table_pages={} reclaimed_user_kernel_stacks={} reclaimed_kernel_stack_writable_pages={} reclaimed_kernel_stack_virtual_pages={}",
        diagnostics.reclaimed_user_resource_records(),
        diagnostics.reclaimed_user_address_spaces(),
        diagnostics.reclaimed_user_pages(),
        diagnostics.reclaimed_user_page_table_pages(),
        diagnostics.reclaimed_user_kernel_stacks(),
        diagnostics.reclaimed_user_kernel_stack_writable_pages(),
        diagnostics.reclaimed_user_kernel_stack_virtual_pages()
    ));
    output.push(format!(
        "user_return: stack_sets={} stack_takes={}",
        diagnostics.user_return_stack_sets(),
        diagnostics.user_return_stack_takes()
    ));
    output.push(format!(
        "process_lifecycle: retained_user_exit_statuses={} waitable_user_exit_statuses={} collected_user_exit_statuses={} zombie_user_tasks={} reaped_user_tasks={}",
        diagnostics.retained_user_exit_statuses(),
        diagnostics.waitable_user_exit_statuses(),
        diagnostics.collected_user_exit_statuses(),
        diagnostics.zombie_user_tasks(),
        diagnostics.reaped_user_tasks()
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
        push_task_snapshot(&mut output, &snapshot);
    }
    Ok(CommandEffect::Output(output))
}

fn push_task_snapshot(output: &mut CommandOutput, snapshot: &SchedulerTaskSnapshot) {
    output.push(format!(
        "task: id={} parent={} kind={} state={} lifecycle={} active={} address_space_owned={} kernel_stack_owned={} exit_code={} wait_collected={} last_preemption_reason={} last_resume_path={}",
        snapshot.task_id(),
        snapshot
            .parent_task_id()
            .map_or_else(|| "-".to_string(), |task_id| task_id.to_string()),
        snapshot.kind().as_str(),
        snapshot.state().as_str(),
        snapshot.process_lifecycle().as_str(),
        snapshot.active(),
        snapshot.address_space_owned(),
        snapshot.kernel_stack_owned(),
        snapshot
            .exit_code()
            .map_or_else(|| "-".to_string(), |exit_code| exit_code.to_string()),
        snapshot.wait_collected(),
        snapshot.last_preemption_reason().as_str(),
        snapshot.last_resume_path().as_str()
    ));
    if let Some(user_image) = snapshot.user_image() {
        push_user_image(output, snapshot.task_id(), user_image);
    }
    if let Some(user_virtual_memory) = snapshot.user_virtual_memory() {
        push_user_virtual_memory(output, snapshot.task_id(), user_virtual_memory);
    }
}

fn push_user_image(
    output: &mut CommandOutput,
    task_id: u64,
    user_image: &UserImageDiagnosticsSnapshot,
) {
    let origin_path =
        path_diagnostic_text(user_image.origin_path_bytes(), user_image.origin_path_len());
    let path_bytes = user_image.path_bytes();
    let path_len = user_image.path_len();
    let image_path = path_diagnostic_text(path_bytes, path_len);
    output.push(format!(
        "task_image: id={} generation={} origin={} path={} last_execve_state={} last_execve_old_user_pages={} last_execve_old_page_table_pages={}",
        task_id,
        user_image.generation(),
        origin_path,
        image_path,
        user_image.last_execve_state().as_str(),
        user_image.last_execve_old_user_pages(),
        user_image.last_execve_old_page_table_pages()
    ));
}

fn path_diagnostic_text(path_bytes: &[u8], path_len: usize) -> &str {
    if path_len == 0 {
        "-"
    } else {
        core::str::from_utf8(&path_bytes[..path_len]).unwrap_or("<invalid>")
    }
}

fn push_user_virtual_memory(
    output: &mut CommandOutput,
    task_id: u64,
    user_virtual_memory: UserVirtualMemorySnapshot,
) {
    output.push(format!(
        "task_vm: id={} heap_base={:#x} heap_break={:#x} heap_pages={} mmap_next={:#x} mmap_pages={} mmap_records={} mmap_file_records={}",
        task_id,
        user_virtual_memory.heap_base(),
        user_virtual_memory.heap_break(),
        user_virtual_memory.heap_mapped_pages(),
        user_virtual_memory.mapping_next_start(),
        user_virtual_memory.mapping_active_pages(),
        user_virtual_memory.mapping_active_records(),
        user_virtual_memory.mapping_file_private_records()
    ));
    output.push(format!(
        "task_mmap_lifecycle: id={} total_mapped_pages={} total_released_pages={} peak_pages={} peak_records={} file_private_maps={}",
        task_id,
        user_virtual_memory.mapping_total_mapped_pages(),
        user_virtual_memory.mapping_total_released_pages(),
        user_virtual_memory.mapping_peak_active_pages(),
        user_virtual_memory.mapping_peak_active_records(),
        user_virtual_memory.mapping_file_private_map_count()
    ));
}
