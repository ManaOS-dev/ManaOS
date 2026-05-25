//! Advanced Host Controller Interface DMA command submission.

use super::dma::{self, AhciDmaBuffers};
use super::registers::{HbaCommandHeader, HbaCommandTable, HbaPort};
use crate::kernel::driver::storage::block_device::SECTOR_BYTES;

const COMMAND_FIS_LENGTH_DWORDS: u16 = 5;
const ATA_COMMAND_READ_DMA_EXT: u8 = 0x25;
const FIS_TYPE_REGISTER_HOST_TO_DEVICE: u8 = 0x27;
const FIS_COMMAND_FLAG: u8 = 1 << 7;
const ATA_DEVICE_LBA_MODE: u8 = 1 << 6;
const TASK_FILE_DATA_BUSY: u32 = 1 << 7;
const TASK_FILE_DATA_DATA_REQUEST: u32 = 1 << 3;
const INTERRUPT_STATUS_TASK_FILE_ERROR: u32 = 1 << 30;
const PORT_POLL_LIMIT: usize = 1_000_000;
const READ_COMMAND_SLOT: u32 = 1;

pub(super) fn issue_read_sector(
    port: *mut HbaPort,
    buffers: AhciDmaBuffers,
    port_index: usize,
    logical_block_address: u64,
) -> bool {
    crate::log_trace!(
        "ahci",
        "Port {}: preparing read command lba={} sector_count=1 data_buffer={:#018x}",
        port_index,
        logical_block_address,
        buffers.data
    );
    if !wait_until_not_busy(port, port_index) {
        return false;
    }

    prepare_read_command(buffers, logical_block_address);

    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port
    // register block.
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).interrupt_status), u32::MAX);
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_issue),
            READ_COMMAND_SLOT,
        );
    }
    crate::log_trace!(
        "ahci",
        "Port {}: command issued for LBA {}",
        port_index,
        logical_block_address
    );

    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface
        // port register block.
        let command_issue =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_issue)) };
        if command_issue & READ_COMMAND_SLOT == 0 {
            // SAFETY: `port` points to a mapped Advanced Host Controller
            // Interface port register block.
            let interrupt_status =
                unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).interrupt_status)) };
            if interrupt_status & INTERRUPT_STATUS_TASK_FILE_ERROR != 0 {
                crate::log_error!(
                    "ahci",
                    "Port {}: read failed, interrupt_status={:#010x}",
                    port_index,
                    interrupt_status
                );
                return false;
            }

            crate::log_debug!(
                "ahci",
                "Read LBA {} complete: bytes={} interrupt_status={:#010x}",
                logical_block_address,
                SECTOR_BYTES,
                interrupt_status
            );
            return true;
        }
    }

    log_timeout_registers(port, port_index, logical_block_address, "read command");
    false
}

fn wait_until_not_busy(port: *mut HbaPort, port_index: usize) -> bool {
    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface
        // port register block.
        let task_file_data =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).task_file_data)) };
        if task_file_data & (TASK_FILE_DATA_BUSY | TASK_FILE_DATA_DATA_REQUEST) == 0 {
            return true;
        }
    }

    log_timeout_registers(port, port_index, 0, "device busy");
    false
}

fn prepare_read_command(buffers: AhciDmaBuffers, logical_block_address: u64) {
    let command_headers = buffers.command_list as *mut HbaCommandHeader;
    let command_table = buffers.command_table as *mut HbaCommandTable;
    let (command_table_low, command_table_high) = dma::split_address(buffers.command_table);
    let (data_low, data_high) = dma::split_address(buffers.data);

    // SAFETY: All pointers refer to zeroed, freshly allocated identity-mapped
    // DMA buffers owned by this Advanced Host Controller Interface command.
    unsafe {
        let header = command_headers;
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*header).flags),
            COMMAND_FIS_LENGTH_DWORDS,
        );
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*header).prdt_length), 1);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*header).prd_byte_count), 0);
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*header).command_table_base),
            command_table_low,
        );
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*header).command_table_base_upper),
            command_table_high,
        );

        let prdt_entry = core::ptr::addr_of_mut!((*command_table).prdt_entries[0]);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*prdt_entry).data_base), data_low);
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*prdt_entry).data_base_upper),
            data_high,
        );
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*prdt_entry).reserved), 0);
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*prdt_entry).byte_count_and_interrupt),
            (u32::try_from(SECTOR_BYTES).expect("sector size must fit in PRDT byte count") - 1)
                | (1 << 31),
        );

        let command_fis = core::ptr::addr_of_mut!((*command_table).command_fis).cast::<u8>();
        core::ptr::write(command_fis.add(0), FIS_TYPE_REGISTER_HOST_TO_DEVICE);
        core::ptr::write(command_fis.add(1), FIS_COMMAND_FLAG);
        core::ptr::write(command_fis.add(2), ATA_COMMAND_READ_DMA_EXT);
        core::ptr::write(command_fis.add(4), lba_byte(logical_block_address, 0));
        core::ptr::write(command_fis.add(5), lba_byte(logical_block_address, 8));
        core::ptr::write(command_fis.add(6), lba_byte(logical_block_address, 16));
        core::ptr::write(command_fis.add(7), ATA_DEVICE_LBA_MODE);
        core::ptr::write(command_fis.add(8), lba_byte(logical_block_address, 24));
        core::ptr::write(command_fis.add(9), lba_byte(logical_block_address, 32));
        core::ptr::write(command_fis.add(10), lba_byte(logical_block_address, 40));
        core::ptr::write(command_fis.add(12), 1);
        core::ptr::write(command_fis.add(13), 0);
    }
}

fn lba_byte(logical_block_address: u64, shift: u32) -> u8 {
    u8::try_from((logical_block_address >> shift) & 0xff).expect("masked LBA byte must fit in u8")
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
        "Port {}: {} timeout lba={} slot_mask={:#010x} task_file_data={:#010x} command_issue={:#010x} sata_active={:#010x} interrupt_status={:#010x} sata_error={:#010x}",
        port_index,
        context,
        logical_block_address,
        READ_COMMAND_SLOT,
        task_file_data,
        command_issue,
        sata_active,
        interrupt_status,
        sata_error
    );
}
