//! Advanced Host Controller Interface command completion modes.

use super::registers::HbaPort;
use crate::kernel::driver::storage::block_device::{BlockDeviceError, BlockDeviceResult};

const TASK_FILE_DATA_BUSY: u32 = 1 << 7;
const TASK_FILE_DATA_DATA_REQUEST: u32 = 1 << 3;
const INTERRUPT_STATUS_TASK_FILE_ERROR: u32 = 1 << 30;
const INTERRUPT_STATUS_DEVICE_TO_HOST_REGISTER_FIS: u32 = 1 << 0;
const PORT_POLL_LIMIT: usize = 1_000_000;

/// Command slot mask used by the current single-slot command path.
pub(super) const COMMAND_SLOT_MASK: u32 = 1;

/// Advanced Host Controller Interface command completion mode.
#[derive(Clone, Copy)]
pub(super) enum CompletionMode {
    /// Poll the command issue bit until completion.
    Polling,
    /// Use port interrupt status bits as the completion signal.
    InterruptDriven,
}

#[derive(Clone, Copy)]
struct CommandCompletion<'a> {
    logical_block_address: u64,
    sector_count: u16,
    transfer_bytes: usize,
    direction: &'a str,
    completion_mode: CompletionMode,
}

impl CompletionMode {
    /// Return a serial-log friendly completion-mode name.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Polling => "polling",
            Self::InterruptDriven => "interrupt-driven",
        }
    }

    /// Return the interrupt-enable mask needed for this completion mode.
    pub(super) fn interrupt_enable(self) -> u32 {
        match self {
            Self::Polling => 0,
            Self::InterruptDriven => {
                INTERRUPT_STATUS_DEVICE_TO_HOST_REGISTER_FIS | INTERRUPT_STATUS_TASK_FILE_ERROR
            }
        }
    }
}

/// Log the completion modes compiled into the AHCI port path.
pub(super) fn log_supported_modes(port_index: usize) {
    crate::log_info!(
        "ahci",
        "Port {}: command completion modes available: {}, {}",
        port_index,
        CompletionMode::Polling.name(),
        CompletionMode::InterruptDriven.name()
    );
}

/// Wait until the AHCI port is ready to accept a command.
pub(super) fn wait_until_not_busy(port: *mut HbaPort, port_index: usize) -> BlockDeviceResult<()> {
    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface
        // port register block.
        let task_file_data =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).task_file_data)) };
        if task_file_data & (TASK_FILE_DATA_BUSY | TASK_FILE_DATA_DATA_REQUEST) == 0 {
            return Ok(());
        }
    }

    log_timeout_registers(port, port_index, 0, "device busy");
    Err(BlockDeviceError::DeviceBusyTimeout)
}

/// Wait for the active command to complete using the configured mode.
pub(super) fn wait_for_command(
    port: *mut HbaPort,
    port_index: usize,
    logical_block_address: u64,
    sector_count: u16,
    transfer_bytes: usize,
    direction: &str,
    completion_mode: CompletionMode,
) -> BlockDeviceResult<()> {
    let completion = CommandCompletion {
        logical_block_address,
        sector_count,
        transfer_bytes,
        direction,
        completion_mode,
    };

    match completion_mode {
        CompletionMode::Polling => wait_for_command_issue_clear(port, port_index, completion),
        CompletionMode::InterruptDriven => wait_for_interrupt_status(port, port_index, completion),
    }
}

fn wait_for_command_issue_clear(
    port: *mut HbaPort,
    port_index: usize,
    completion: CommandCompletion<'_>,
) -> BlockDeviceResult<()> {
    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface
        // port register block.
        let command_issue =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_issue)) };
        if command_issue & COMMAND_SLOT_MASK == 0 {
            return finish_completed_command(port, port_index, completion, "command-issue");
        }
    }

    log_timeout_registers(
        port,
        port_index,
        completion.logical_block_address,
        completion.direction,
    );
    Err(BlockDeviceError::CommandTimeout)
}

fn wait_for_interrupt_status(
    port: *mut HbaPort,
    port_index: usize,
    completion: CommandCompletion<'_>,
) -> BlockDeviceResult<()> {
    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface
        // port register block.
        let interrupt_status =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).interrupt_status)) };
        if interrupt_status & INTERRUPT_STATUS_TASK_FILE_ERROR != 0 {
            log_task_file_error(port, port_index, completion.direction, interrupt_status);
            return Err(BlockDeviceError::TaskFileError);
        }
        if interrupt_status & INTERRUPT_STATUS_DEVICE_TO_HOST_REGISTER_FIS != 0 {
            return finish_completed_command(port, port_index, completion, "interrupt-status");
        }
    }

    log_timeout_registers(
        port,
        port_index,
        completion.logical_block_address,
        completion.direction,
    );
    Err(BlockDeviceError::CommandTimeout)
}

fn finish_completed_command(
    port: *mut HbaPort,
    port_index: usize,
    completion: CommandCompletion<'_>,
    completion_source: &str,
) -> BlockDeviceResult<()> {
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let interrupt_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).interrupt_status)) };
    if interrupt_status & INTERRUPT_STATUS_TASK_FILE_ERROR != 0 {
        log_task_file_error(port, port_index, completion.direction, interrupt_status);
        return Err(BlockDeviceError::TaskFileError);
    }

    crate::log_debug!(
        "ahci",
        "{} LBA {} complete: sectors={} bytes={} interrupt_status={:#010x} completion_mode={} completion_source={}",
        completion.direction,
        completion.logical_block_address,
        completion.sector_count,
        completion.transfer_bytes,
        interrupt_status,
        completion.completion_mode.name(),
        completion_source
    );
    Ok(())
}

fn log_task_file_error(
    port: *mut HbaPort,
    port_index: usize,
    direction: &str,
    interrupt_status: u32,
) {
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let sata_error = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).sata_error)) };
    crate::log_error!(
        "ahci",
        "Port {}: {} failed, interrupt_status={:#010x} sata_error={:#010x}",
        port_index,
        direction,
        interrupt_status,
        sata_error
    );
}

fn log_timeout_registers(
    port: *mut HbaPort,
    port_index: usize,
    logical_block_address: u64,
    context: &str,
) {
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let task_file_data =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).task_file_data)) };
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let command_issue =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_issue)) };
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let sata_active = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).sata_active)) };
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let interrupt_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).interrupt_status)) };
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let sata_error = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).sata_error)) };
    crate::log_error!(
        "ahci",
        "Port {}: {} command timeout lba={} slot_mask={:#010x} task_file_data={:#010x} command_issue={:#010x} sata_active={:#010x} interrupt_status={:#010x} sata_error={:#010x} d2h_fis_seen={}",
        port_index,
        context,
        logical_block_address,
        COMMAND_SLOT_MASK,
        task_file_data,
        command_issue,
        sata_active,
        interrupt_status,
        sata_error,
        interrupt_status & INTERRUPT_STATUS_DEVICE_TO_HOST_REGISTER_FIS != 0
    );
}
