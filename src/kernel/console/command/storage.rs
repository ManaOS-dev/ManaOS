//! `storage` and `partitions` kernel console commands.

use super::output::{CommandEffect, CommandError, CommandOutput};
use alloc::string::ToString;

pub(super) fn run(
    arguments: &str,
    _input: &[alloc::string::String],
) -> Result<CommandEffect, CommandError> {
    if !arguments.is_empty() {
        return Err(CommandError::UnknownCommand);
    }

    let mut output = CommandOutput::new();
    if let Some(partition) = crate::kernel::driver::storage::get_selected_partition() {
        output.push(alloc::format!(
            "partition {}: first_lba={} last_lba={} name=\"{}\"",
            partition.index,
            partition.first_lba,
            partition.last_lba,
            partition.name()
        ));
    } else {
        output.push("storage: no selected GPT partition".to_string());
    }

    let devices = crate::kernel::driver::storage::get_storage_devices();
    if devices.is_empty() {
        output.push("storage: no registered block devices".to_string());
        return Ok(CommandEffect::Output(output));
    }

    for device in devices {
        output.push(alloc::format!(
            "device {}: sector_size={} max_transfer={}",
            device.id,
            device.sector_size,
            device.maximum_transfer_sectors
        ));
    }
    Ok(CommandEffect::Output(output))
}
