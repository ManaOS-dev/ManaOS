//! Advanced Host Controller Interface controller helpers.

use core::fmt;

use crate::kernel::memory::{frame_allocator::BumpFrameAllocator, paging};

use super::super::block_device::{BlockDevice, SECTOR_BYTES};
use super::super::{
    file_allocation_table, guid_partition_table, partition::PartitionBlockDevice,
    set_selected_partition,
};
use super::registers::{HbaCommandHeader, HbaCommandTable, HbaMemory, HbaPort, MAX_PORTS};

const HOST_BUS_ADAPTER_MEMORY_SIZE: u64 = 0x1100;
const SATA_SIGNATURE: u32 = 0x0000_0101;
const DMA_PAGE_SIZE: usize = 4096;
const COMMAND_FIS_LENGTH_DWORDS: u16 = 5;
const ATA_COMMAND_READ_DMA_EXT: u8 = 0x25;
const FIS_TYPE_REGISTER_HOST_TO_DEVICE: u8 = 0x27;
const FIS_COMMAND_FLAG: u8 = 1 << 7;
const ATA_DEVICE_LBA_MODE: u8 = 1 << 6;
const COMMAND_AND_STATUS_START: u32 = 1 << 0;
const COMMAND_AND_STATUS_FIS_RECEIVE_ENABLE: u32 = 1 << 4;
const COMMAND_AND_STATUS_FIS_RECEIVE_RUNNING: u32 = 1 << 14;
const COMMAND_AND_STATUS_COMMAND_LIST_RUNNING: u32 = 1 << 15;
const TASK_FILE_DATA_BUSY: u32 = 1 << 7;
const TASK_FILE_DATA_DATA_REQUEST: u32 = 1 << 3;
const GLOBAL_HOST_CONTROL_AHCI_ENABLE: u32 = 1 << 31;
const INTERRUPT_STATUS_TASK_FILE_ERROR: u32 = 1 << 30;
const PORT_POLL_LIMIT: usize = 1_000_000;

#[derive(Clone, Copy)]
struct AhciDmaBuffers {
    command_list: u64,
    received_fis: u64,
    command_table: u64,
    data: u64,
}

struct AhciBlockDevice {
    port: *mut HbaPort,
    buffers: AhciDmaBuffers,
    port_index: usize,
}

impl AhciBlockDevice {
    fn new(port: *mut HbaPort, buffers: AhciDmaBuffers, port_index: usize) -> Self {
        Self {
            port,
            buffers,
            port_index,
        }
    }
}

impl BlockDevice for AhciBlockDevice {
    fn read_logical_block(&mut self, logical_block_address: u64, data_address: u64) -> bool {
        if data_address != self.buffers.data {
            crate::log_error!(
                "ahci",
                "unexpected AHCI read buffer: requested={:#018x} owned={:#018x}",
                data_address,
                self.buffers.data
            );
            return false;
        }

        issue_read_sector(
            self.port,
            self.buffers,
            self.port_index,
            logical_block_address,
        )
    }
}

/// Initialize an Advanced Host Controller Interface controller from its base address register 5 MMIO base.
pub fn init(frame_allocator: &mut BumpFrameAllocator, base_address_register5: u64) {
    crate::log_debug!(
        "ahci",
        "Mapping HBA MMIO: base={:#010x} size={:#x}",
        base_address_register5,
        HOST_BUS_ADAPTER_MEMORY_SIZE
    );
    // SAFETY: base address register 5 is reported by a PCI mass-storage SATA controller and points
    // to the Advanced Host Controller Interface host bus adapter MMIO register block.
    unsafe {
        paging::map_kernel_mmio_range(
            frame_allocator,
            base_address_register5,
            HOST_BUS_ADAPTER_MEMORY_SIZE,
        );
    }
    crate::log_debug!("ahci", "HBA MMIO mapping complete.");

    let hba_memory = base_address_register5 as *mut HbaMemory;
    enable_ahci(hba_memory);
    // SAFETY: The base address register 5 MMIO range was mapped above, and `hba_memory` points to
    // the Advanced Host Controller Interface host bus adapter register block.
    let ports_implemented =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).ports_implemented)) };
    crate::log_info!("ahci", "HBA ports implemented: {:#010x}", ports_implemented);

    for port_index in 0..MAX_PORTS {
        if ports_implemented & (1_u32 << port_index) == 0 {
            continue;
        }

        // SAFETY: `port_index` is below MAX_PORTS and this port is reported in
        // the host bus adapter ports implemented bitmap.
        let port = unsafe {
            let ports = core::ptr::addr_of!((*hba_memory).ports).cast::<HbaPort>();
            ports.add(port_index)
        };
        log_port_registers(port_index, port);

        // SAFETY: `port` points to an implemented mapped Advanced Host Controller Interface port.
        let signature = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).signature)) };
        if signature == SATA_SIGNATURE {
            crate::log_info!("ahci", "Port {}: SATA device detected", port_index);
            if let Some(buffers) = allocate_dma_buffers(frame_allocator) {
                read_initial_sectors(hba_memory, port_index, buffers);
            } else {
                crate::log_error!(
                    "ahci",
                    "Port {}: failed to allocate DMA buffers",
                    port_index
                );
            }
            return;
        }

        crate::log_debug!(
            "ahci",
            "Port {}: non-SATA signature {:#010x}",
            port_index,
            signature
        );
    }

    crate::log_warn!("ahci", "No usable SATA port found.");
}

fn enable_ahci(hba_memory: *mut HbaMemory) {
    // SAFETY: `hba_memory` points to the mapped Advanced Host Controller Interface host bus adapter register block.
    let global_host_control =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).global_host_control)) };
    crate::log_debug!(
        "ahci",
        "Global host control before enable: {:#010x}",
        global_host_control
    );
    // SAFETY: `hba_memory` points to the mapped Advanced Host Controller Interface host bus adapter register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*hba_memory).global_host_control),
            global_host_control | GLOBAL_HOST_CONTROL_AHCI_ENABLE,
        );
    }
    // SAFETY: `hba_memory` points to the mapped Advanced Host Controller Interface host bus adapter register block.
    let enabled_global_host_control =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).global_host_control)) };
    crate::log_debug!(
        "ahci",
        "Global host control after enable: {:#010x}",
        enabled_global_host_control
    );
}

fn allocate_dma_buffers(frame_allocator: &mut BumpFrameAllocator) -> Option<AhciDmaBuffers> {
    let command_list = frame_allocator.allocate_frame()?;
    let received_fis = frame_allocator.allocate_frame()?;
    let command_table = frame_allocator.allocate_frame()?;
    let data = frame_allocator.allocate_frame()?;

    zero_page(command_list);
    zero_page(received_fis);
    zero_page(command_table);
    zero_page(data);

    crate::log_debug!(
        "ahci",
        "DMA buffers: command_list={:#018x} received_fis={:#018x} command_table={:#018x} data={:#018x}",
        command_list,
        received_fis,
        command_table,
        data
    );

    Some(AhciDmaBuffers {
        command_list,
        received_fis,
        command_table,
        data,
    })
}

fn zero_page(physical_address: u64) {
    let pointer = physical_address as *mut u8;
    // SAFETY: DMA buffers come from freshly allocated identity-mapped frames.
    unsafe {
        core::ptr::write_bytes(pointer, 0, DMA_PAGE_SIZE);
    }
}

fn read_initial_sectors(hba_memory: *mut HbaMemory, port_index: usize, buffers: AhciDmaBuffers) {
    // SAFETY: `port_index` is within MAX_PORTS and was reported implemented by
    // the host bus adapter before this helper is called.
    let port = unsafe {
        let ports = core::ptr::addr_of_mut!((*hba_memory).ports).cast::<HbaPort>();
        ports.add(port_index)
    };

    if !stop_command_engine(port, port_index) {
        return;
    }

    rebase_port(port, buffers);
    start_command_engine(port);
    crate::log_info!(
        "ahci",
        "Port {}: command engine started; command slots available",
        port_index
    );

    let mut block_device = AhciBlockDevice::new(port, buffers, port_index);

    if block_device.read_logical_block(0, buffers.data) {
        dump_sector_prefix("LBA0", buffers.data);
    }

    if block_device.read_logical_block(1, buffers.data) {
        if let Some(header) = guid_partition_table::inspect_header(buffers.data) {
            if let Some(partition) = guid_partition_table::inspect_partition_table(
                &mut block_device,
                header,
                buffers.data,
            ) {
                crate::log_info!(
                    "storage",
                    "Selected GPT partition: index={} first_lba={} last_lba={} name=\"{}\"",
                    partition.index,
                    partition.first_lba,
                    partition.last_lba,
                    partition.name()
                );
                set_selected_partition(partition);
                let mut partition_device = PartitionBlockDevice::new(
                    &mut block_device,
                    partition.first_lba,
                    partition.last_lba,
                );
                if let Some(volume) =
                    file_allocation_table::inspect_boot_sector(&mut partition_device, buffers.data)
                {
                    let _ = file_allocation_table::inspect_root_directory(
                        &mut partition_device,
                        volume,
                        buffers.data,
                    );
                }
            }
        }
    }
}

fn stop_command_engine(port: *mut HbaPort, port_index: usize) -> bool {
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
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
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_and_status),
            command_and_status,
        );
    }

    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
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
    let (command_list_low, command_list_high) = split_address(buffers.command_list);
    let (received_fis_low, received_fis_high) = split_address(buffers.received_fis);

    crate::log_debug!(
        "ahci",
        "Rebasing port: command_list=({:#010x},{:#010x}) received_fis=({:#010x},{:#010x})",
        command_list_high,
        command_list_low,
        received_fis_high,
        received_fis_low
    );

    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block, and all
    // buffer addresses are freshly allocated physical frames.
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
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
    let command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    crate::log_debug!(
        "ahci",
        "Starting command engine: command_and_status before={:#010x}",
        command_and_status
    );
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_and_status),
            command_and_status | COMMAND_AND_STATUS_FIS_RECEIVE_ENABLE | COMMAND_AND_STATUS_START,
        );
    }
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
    let started_command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    crate::log_debug!(
        "ahci",
        "Command engine start requested: command_and_status after={:#010x}",
        started_command_and_status
    );
}

fn issue_read_sector(
    port: *mut HbaPort,
    buffers: AhciDmaBuffers,
    port_index: usize,
    lba: u64,
) -> bool {
    crate::log_trace!(
        "ahci",
        "Port {}: preparing read command lba={} sector_count=1 data_buffer={:#018x}",
        port_index,
        lba,
        buffers.data
    );
    if !wait_until_not_busy(port, port_index) {
        return false;
    }

    prepare_read_command(buffers, lba);

    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).interrupt_status), u32::MAX);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).command_issue), 1);
    }
    crate::log_trace!(
        "ahci",
        "Port {}: command issued for LBA {}",
        port_index,
        lba
    );

    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
        let command_issue =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_issue)) };
        if command_issue & 1 == 0 {
            // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
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
                lba,
                SECTOR_BYTES,
                interrupt_status
            );
            return true;
        }
    }

    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
    let task_file_data =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).task_file_data)) };
    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
    let command_issue =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_issue)) };
    crate::log_error!(
        "ahci",
        "Port {}: read LBA {} timeout, task_file_data={:#010x} command_issue={:#010x}",
        port_index,
        lba,
        task_file_data,
        command_issue
    );
    false
}

fn wait_until_not_busy(port: *mut HbaPort, port_index: usize) -> bool {
    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
        let task_file_data =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).task_file_data)) };
        if task_file_data & (TASK_FILE_DATA_BUSY | TASK_FILE_DATA_DATA_REQUEST) == 0 {
            return true;
        }
    }

    // SAFETY: `port` points to a mapped Advanced Host Controller Interface port register block.
    let task_file_data =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).task_file_data)) };
    crate::log_error!(
        "ahci",
        "Port {}: device busy timeout, task_file_data={:#010x}",
        port_index,
        task_file_data
    );
    false
}

fn prepare_read_command(buffers: AhciDmaBuffers, lba: u64) {
    let command_headers = buffers.command_list as *mut HbaCommandHeader;
    let command_table = buffers.command_table as *mut HbaCommandTable;
    let (command_table_low, command_table_high) = split_address(buffers.command_table);
    let (data_low, data_high) = split_address(buffers.data);

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
        core::ptr::write(command_fis.add(4), lba_byte(lba, 0));
        core::ptr::write(command_fis.add(5), lba_byte(lba, 8));
        core::ptr::write(command_fis.add(6), lba_byte(lba, 16));
        core::ptr::write(command_fis.add(7), ATA_DEVICE_LBA_MODE);
        core::ptr::write(command_fis.add(8), lba_byte(lba, 24));
        core::ptr::write(command_fis.add(9), lba_byte(lba, 32));
        core::ptr::write(command_fis.add(10), lba_byte(lba, 40));
        core::ptr::write(command_fis.add(12), 1);
        core::ptr::write(command_fis.add(13), 0);
    }
}

fn lba_byte(lba: u64, shift: u32) -> u8 {
    u8::try_from((lba >> shift) & 0xff).expect("masked LBA byte must fit in u8")
}

fn dump_sector_prefix(label: &str, data_address: u64) {
    crate::log_debug!("ahci", "{}: {}", label, SectorPrefix { data_address });
}

struct SectorPrefix {
    data_address: u64,
}

impl fmt::Display for SectorPrefix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let data = self.data_address as *const u8;
        for offset in 0..16 {
            // SAFETY: `data_address` points to a 512-byte DMA read buffer.
            let byte = unsafe { core::ptr::read_volatile(data.add(offset)) };
            write!(formatter, " {byte:02x}")?;
        }
        Ok(())
    }
}

fn log_port_registers(port_index: usize, port: *const HbaPort) {
    // SAFETY: `port` points to an implemented mapped Advanced Host Controller Interface port.
    let signature = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).signature)) };
    // SAFETY: `port` points to an implemented mapped Advanced Host Controller Interface port.
    let sata_status = unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).sata_status)) };
    // SAFETY: `port` points to an implemented mapped Advanced Host Controller Interface port.
    let command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    // SAFETY: `port` points to an implemented mapped Advanced Host Controller Interface port.
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

fn split_address(address: u64) -> (u32, u32) {
    (
        u32::try_from(address & 0xffff_ffff).expect("low address bits must fit in u32"),
        u32::try_from(address >> 32).expect("high address bits must fit in u32"),
    )
}
