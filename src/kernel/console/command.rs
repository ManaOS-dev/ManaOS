//! Kernel console command parsing and dispatch.

use alloc::format;
use alloc::string::{String, ToString};

pub(super) fn execute(command: &str) {
    if command.is_empty() {
        return;
    }

    super::push_output(format!("> {command}"));
    let (name, argument) = split_command(command);
    match name {
        "help" => super::push_output(
            "commands: help clear ticks fps storage partitions cat read echo syscalls".to_string(),
        ),
        "clear" if argument.is_empty() => super::clear_output(),
        "ticks" if argument.is_empty() => {
            super::push_output(format!("ticks={}", crate::kernel::time::get_timer_ticks()));
        }
        "fps" if argument.is_empty() => {
            super::push_output(format!("fps={}", crate::kernel::runtime::get_fps()));
        }
        "storage" | "partitions" if argument.is_empty() => push_storage_output(),
        "cat" | "read" => push_file_output(name, argument),
        "echo" => super::push_output(argument.to_string()),
        "syscalls" if argument.is_empty() => {
            super::push_output(
                "syscalls: read write open close exit exit_group openat getpid".to_string(),
            );
        }
        _ => super::push_output(format!("unknown command: {command}")),
    }
}

fn split_command(command: &str) -> (&str, &str) {
    command
        .split_once(' ')
        .map_or((command, ""), |(name, argument)| (name, argument.trim()))
}

fn push_file_output(command_name: &str, path: &str) {
    if path.is_empty() {
        super::push_output(format!("usage: {command_name} /path"));
        return;
    }

    let Ok(file_descriptor) = crate::kernel::filesystem::open(path) else {
        super::push_output(format!("{command_name}: cannot open {path}"));
        return;
    };

    let mut buffer = [0_u8; 80];
    let result = crate::kernel::filesystem::read(file_descriptor, &mut buffer);
    let _ = crate::kernel::filesystem::close(file_descriptor);
    let Ok(bytes_read) = result else {
        super::push_output(format!("{command_name}: cannot read {path}"));
        return;
    };

    super::push_output(format_file_contents(&buffer[..bytes_read]));
}

fn format_file_contents(bytes: &[u8]) -> String {
    let mut output = String::new();
    for byte in bytes {
        match *byte {
            b'\n' | b'\r' => break,
            0x20..=0x7e => output.push(char::from(*byte)),
            _ => output.push('.'),
        }
    }

    output
}

fn push_storage_output() {
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
