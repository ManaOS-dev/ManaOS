//! Advanced Host Controller Interface port lifecycle helpers.

use super::dma::{self, AhciDmaBuffers};
use super::registers::HbaPort;

const COMMAND_AND_STATUS_START: u32 = 1 << 0;
const COMMAND_AND_STATUS_FIS_RECEIVE_ENABLE: u32 = 1 << 4;
const COMMAND_AND_STATUS_FIS_RECEIVE_RUNNING: u32 = 1 << 14;
const COMMAND_AND_STATUS_COMMAND_LIST_RUNNING: u32 = 1 << 15;
const PORT_POLL_LIMIT: usize = 1_000_000;

pub(super) fn read_signature(port: *const HbaPort) -> u32 {
    // SAFETY: `port` points to an implemented mapped Advanced Host Controller
    // Interface port.
    unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).signature)) }
}

pub(super) fn log_registers(port_index: usize, port: *const HbaPort) {
    let signature = read_signature(port);
    // SAFETY: `port` points to an implemented mapped Advanced Host Controller
    // Interface port.
    let sata_status = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).sata_status)) };
    // SAFETY: `port` points to an implemented mapped Advanced Host Controller
    // Interface port.
    let command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    // SAFETY: `port` points to an implemented mapped Advanced Host Controller
    // Interface port.
    let task_file_data =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).task_file_data)) };
    crate::log_debug!(
        "ahci",
        "Port {} registers: signature={:#010x} sata_status={:#010x} command_and_status={:#010x} task_file_data={:#010x}",
        port_index,
        signature,
        sata_status,
        command_and_status,
        task_file_data
    );
}

pub(super) fn initialize_command_engine(
    port: *mut HbaPort,
    port_index: usize,
    buffers: AhciDmaBuffers,
) -> bool {
    if !stop_command_engine(port, port_index) {
        return false;
    }

    rebase_port(port, buffers);
    start_command_engine(port);
    crate::log_info!(
        "ahci",
        "Port {}: command engine started; command slots available",
        port_index
    );
    true
}

fn stop_command_engine(port: *mut HbaPort, port_index: usize) -> bool {
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    crate::log_debug!(
        "ahci",
        "Port {}: stopping command engine, command_and_status before={:#010x}",
        port_index,
        command_and_status
    );
    let command_and_status =
        command_and_status & !(COMMAND_AND_STATUS_START | COMMAND_AND_STATUS_FIS_RECEIVE_ENABLE);
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_and_status),
            command_and_status,
        );
    }

    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface
        // port register block.
        let value =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
        if value
            & (COMMAND_AND_STATUS_FIS_RECEIVE_RUNNING | COMMAND_AND_STATUS_COMMAND_LIST_RUNNING)
            == 0
        {
            crate::log_debug!(
                "ahci",
                "Port {}: command engine stopped, command_and_status={:#010x}",
                port_index,
                value
            );
            return true;
        }
    }

    crate::log_error!(
        "ahci",
        "Port {}: timeout while stopping command engine",
        port_index
    );
    false
}

fn rebase_port(port: *mut HbaPort, buffers: AhciDmaBuffers) {
    let (command_list_low, command_list_high) = dma::split_address(buffers.command_list);
    let (received_fis_low, received_fis_high) = dma::split_address(buffers.received_fis);

    crate::log_debug!(
        "ahci",
        "Rebasing port: command_list=({:#010x},{:#010x}) received_fis=({:#010x},{:#010x})",
        command_list_high,
        command_list_low,
        received_fis_high,
        received_fis_low
    );

    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block, and all buffer addresses are freshly allocated physical
    // frames.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_list_base),
            command_list_low,
        );
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_list_base_upper),
            command_list_high,
        );
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).fis_base), received_fis_low);
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).fis_base_upper),
            received_fis_high,
        );
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).interrupt_status), u32::MAX);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).sata_error), u32::MAX);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).interrupt_enable), 0);
    }
}

fn start_command_engine(port: *mut HbaPort) {
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    crate::log_debug!(
        "ahci",
        "Starting command engine: command_and_status before={:#010x}",
        command_and_status
    );
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_and_status),
            command_and_status | COMMAND_AND_STATUS_FIS_RECEIVE_ENABLE | COMMAND_AND_STATUS_START,
        );
    }
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    let started_command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    crate::log_debug!(
        "ahci",
        "Command engine start requested: command_and_status after={:#010x}",
        started_command_and_status
    );
}
