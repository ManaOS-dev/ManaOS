//! `memory` kernel console command.

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

    let Some(diagnostics) = crate::kernel::memory::diagnostics::get_frame_allocator_diagnostics()
    else {
        return Ok(CommandEffect::Output(CommandOutput::single(
            "memory: frame allocator diagnostics unavailable".to_string(),
        )));
    };
    let owners = diagnostics.owners();

    let mut output = CommandOutput::new();
    output.push(format!(
        "memory: total_pages={} free_pages={} used_pages={} reserved_pages={}",
        diagnostics.total(),
        diagnostics.free(),
        diagnostics.used(),
        diagnostics.reserved()
    ));
    output.push(format!(
        "owners: firmware_reserved={} kernel_image={} mmio={} page_table={} kernel_heap={} kernel_stack={} framebuffer_backbuffer={} ahci_dma={} dynamic_kernel_mapping={}",
        owners.firmware_reserved(),
        owners.kernel_image(),
        owners.mmio(),
        owners.page_table(),
        owners.kernel_heap(),
        owners.kernel_stack(),
        owners.framebuffer_backbuffer(),
        owners.ahci_dma(),
        owners.dynamic_kernel_mapping()
    ));
    output.push(format!(
        "user_memory: user_pages={} user_stack={} user_elf={} user_heap={} guard_pages={} unknown_used={} owner_free={}",
        owners.user_pages(),
        owners.user_stack(),
        owners.user_elf(),
        owners.user_heap(),
        owners.guard_page(),
        owners.unknown_used(),
        owners.free()
    ));
    Ok(CommandEffect::Output(output))
}
