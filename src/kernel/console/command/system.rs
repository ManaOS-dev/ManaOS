//! System diagnostic kernel console commands.

use alloc::format;
use alloc::string::ToString;

pub(super) fn push_ticks_output() {
    super::push_output(format!("ticks={}", crate::kernel::time::get_timer_ticks()));
}

pub(super) fn push_fps_output() {
    super::push_output(format!("fps={}", crate::kernel::runtime::get_fps()));
}

pub(super) fn push_syscalls_output() {
    super::push_output("syscalls: read write open close exit exit_group openat getpid".to_string());
}

pub(super) fn push_storage_output() {
    if let Some(partition) = crate::kernel::driver::storage::get_selected_partition() {
        super::push_output(format!(
            "partition {}: first_lba={} last_lba={} name=\"{}\"",
            partition.index,
            partition.first_lba,
            partition.last_lba,
            partition.name()
        ));
    } else {
        super::push_output("storage: no selected GPT partition".to_string());
    }

    let devices = crate::kernel::driver::storage::get_storage_devices();
    if devices.is_empty() {
        super::push_output("storage: no registered block devices".to_string());
        return;
    }

    for device in devices {
        super::push_output(format!(
            "device {}: sector_size={} max_transfer={}",
            device.id, device.sector_size, device.maximum_transfer_sectors
        ));
    }
}
