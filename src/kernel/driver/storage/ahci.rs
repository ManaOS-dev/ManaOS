//! AHCI controller discovery helpers.

use crate::kernel::memory::{frame_allocator::BumpFrameAllocator, paging};

const HBA_MEMORY_SIZE: u64 = 0x1100;
const MAX_PORTS: usize = 32;
const SATA_SIGNATURE: u32 = 0x0000_0101;
const READ_SECTOR_BYTES: usize = 512;
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

/// AHCI host bus adapter memory registers.
#[repr(C)]
pub struct HbaMemory {
    host_capability: u32,
    global_host_control: u32,
    interrupt_status: u32,
    ports_implemented: u32,
    version: u32,
    command_completion_coalescing_control: u32,
    command_completion_coalescing_ports: u32,
    enclosure_management_location: u32,
    enclosure_management_control: u32,
    host_capability_extended: u32,
    bios_os_handoff_control_status: u32,
    reserved: [u8; 0x74],
    vendor: [u8; 0x60],
    ports: [HbaPort; MAX_PORTS],
}

/// AHCI port register block.
#[repr(C)]
pub struct HbaPort {
    command_list_base: u32,
    command_list_base_upper: u32,
    fis_base: u32,
    fis_base_upper: u32,
    interrupt_status: u32,
    interrupt_enable: u32,
    command_and_status: u32,
    reserved0: u32,
    task_file_data: u32,
    signature: u32,
    sata_status: u32,
    sata_control: u32,
    sata_error: u32,
    sata_active: u32,
    command_issue: u32,
    sata_notification: u32,
    fis_based_switching_control: u32,
    reserved1: [u32; 11],
    vendor: [u32; 4],
}

#[repr(C)]
struct HbaCommandHeader {
    flags: u16,
    prdt_length: u16,
    prd_byte_count: u32,
    command_table_base: u32,
    command_table_base_upper: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct HbaPrdtEntry {
    data_base: u32,
    data_base_upper: u32,
    reserved: u32,
    byte_count_and_interrupt: u32,
}

#[repr(C)]
struct HbaCommandTable {
    command_fis: [u8; 64],
    atapi_command: [u8; 16],
    reserved: [u8; 48],
    prdt_entries: [HbaPrdtEntry; 1],
}

#[derive(Clone, Copy)]
struct AhciDmaBuffers {
    command_list: u64,
    received_fis: u64,
    command_table: u64,
    data: u64,
}

/// Initialize an AHCI controller from its BAR5 MMIO base.
pub fn init(frame_allocator: &mut BumpFrameAllocator, bar5: u64) {
    // SAFETY: BAR5 is reported by a PCI mass-storage SATA controller and points
    // to the AHCI HBA MMIO register block.
    unsafe {
        paging::map_kernel_mmio_range(frame_allocator, bar5, HBA_MEMORY_SIZE);
    }

    let hba_memory = bar5 as *mut HbaMemory;
    enable_ahci(hba_memory);
    // SAFETY: The BAR5 MMIO range was mapped above, and `hba_memory` points to
    // the AHCI HBA register block.
    let ports_implemented =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).ports_implemented)) };
    crate::serial_println!("[ahci ] HBA ports implemented: {:#010x}", ports_implemented);

    for port_index in 0..MAX_PORTS {
        if ports_implemented & (1_u32 << port_index) == 0 {
            continue;
        }

        // SAFETY: `port_index` is below MAX_PORTS and this port is reported in
        // the HBA ports implemented bitmap.
        let signature = unsafe {
            let ports = core::ptr::addr_of!((*hba_memory).ports).cast::<HbaPort>();
            let port = ports.add(port_index);
            core::ptr::read_volatile(core::ptr::addr_of!((*port).signature))
        };
        if signature == SATA_SIGNATURE {
            crate::serial_println!("[ahci ] Port {}: SATA device detected", port_index);
            if let Some(buffers) = allocate_dma_buffers(frame_allocator) {
                read_lba0(hba_memory, port_index, buffers);
            } else {
                crate::serial_println!(
                    "[ahci ] Port {}: failed to allocate DMA buffers",
                    port_index
                );
            }
            break;
        }

        crate::serial_println!(
            "[ahci ] Port {}: non-SATA signature {:#010x}",
            port_index,
            signature
        );
    }
}

fn enable_ahci(hba_memory: *mut HbaMemory) {
    // SAFETY: `hba_memory` points to the mapped AHCI HBA register block.
    let global_host_control =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).global_host_control)) };
    // SAFETY: `hba_memory` points to the mapped AHCI HBA register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*hba_memory).global_host_control),
            global_host_control | GLOBAL_HOST_CONTROL_AHCI_ENABLE,
        );
    }
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

fn read_lba0(hba_memory: *mut HbaMemory, port_index: usize, buffers: AhciDmaBuffers) {
    // SAFETY: `port_index` is within MAX_PORTS and was reported implemented by
    // the HBA before this helper is called.
    let port = unsafe {
        let ports = core::ptr::addr_of_mut!((*hba_memory).ports).cast::<HbaPort>();
        ports.add(port_index)
    };

    if !stop_command_engine(port, port_index) {
        return;
    }

    rebase_port(port, buffers);
    start_command_engine(port);
    crate::serial_println!("[ahci ] Port {}: command slots available", port_index);

    if issue_read_lba0(port, buffers, port_index) {
        dump_lba0(buffers.data);
    }
}

fn stop_command_engine(port: *mut HbaPort, port_index: usize) -> bool {
    // SAFETY: `port` points to a mapped AHCI port register block.
    let command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    let command_and_status =
        command_and_status & !(COMMAND_AND_STATUS_START | COMMAND_AND_STATUS_FIS_RECEIVE_ENABLE);
    // SAFETY: `port` points to a mapped AHCI port register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_and_status),
            command_and_status,
        );
    }

    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped AHCI port register block.
        let value =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
        if value
            & (COMMAND_AND_STATUS_FIS_RECEIVE_RUNNING | COMMAND_AND_STATUS_COMMAND_LIST_RUNNING)
            == 0
        {
            crate::serial_println!("[ahci ] Port {}: command engine stopped", port_index);
            return true;
        }
    }

    crate::serial_println!(
        "[ahci ] Port {}: timeout while stopping command engine",
        port_index
    );
    false
}

fn rebase_port(port: *mut HbaPort, buffers: AhciDmaBuffers) {
    let (command_list_low, command_list_high) = split_address(buffers.command_list);
    let (received_fis_low, received_fis_high) = split_address(buffers.received_fis);

    // SAFETY: `port` points to a mapped AHCI port register block, and all
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
    // SAFETY: `port` points to a mapped AHCI port register block.
    let command_and_status =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_and_status)) };
    // SAFETY: `port` points to a mapped AHCI port register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*port).command_and_status),
            command_and_status | COMMAND_AND_STATUS_FIS_RECEIVE_ENABLE | COMMAND_AND_STATUS_START,
        );
    }
}

fn issue_read_lba0(port: *mut HbaPort, buffers: AhciDmaBuffers, port_index: usize) -> bool {
    if !wait_until_not_busy(port, port_index) {
        return false;
    }

    prepare_read_command(buffers);

    // SAFETY: `port` points to a mapped AHCI port register block.
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).interrupt_status), u32::MAX);
        core::ptr::write_volatile(core::ptr::addr_of_mut!((*port).command_issue), 1);
    }

    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped AHCI port register block.
        let command_issue =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).command_issue)) };
        if command_issue & 1 == 0 {
            // SAFETY: `port` points to a mapped AHCI port register block.
            let interrupt_status =
                unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).interrupt_status)) };
            if interrupt_status & INTERRUPT_STATUS_TASK_FILE_ERROR != 0 {
                crate::serial_println!(
                    "[ahci ] Port {}: read failed, interrupt_status={:#010x}",
                    port_index,
                    interrupt_status
                );
                return false;
            }

            crate::serial_println!("[ahci ] Read LBA 0: {} bytes", READ_SECTOR_BYTES);
            return true;
        }
    }

    crate::serial_println!("[ahci ] Port {}: read LBA0 timeout", port_index);
    false
}

fn wait_until_not_busy(port: *mut HbaPort, port_index: usize) -> bool {
    for _ in 0..PORT_POLL_LIMIT {
        // SAFETY: `port` points to a mapped AHCI port register block.
        let task_file_data =
            unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*port).task_file_data)) };
        if task_file_data & (TASK_FILE_DATA_BUSY | TASK_FILE_DATA_DATA_REQUEST) == 0 {
            return true;
        }
    }

    crate::serial_println!("[ahci ] Port {}: device busy timeout", port_index);
    false
}

fn prepare_read_command(buffers: AhciDmaBuffers) {
    let command_headers = buffers.command_list as *mut HbaCommandHeader;
    let command_table = buffers.command_table as *mut HbaCommandTable;
    let (command_table_low, command_table_high) = split_address(buffers.command_table);
    let (data_low, data_high) = split_address(buffers.data);

    // SAFETY: All pointers refer to zeroed, freshly allocated identity-mapped
    // DMA buffers owned by this AHCI command.
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
            (u32::try_from(READ_SECTOR_BYTES).expect("sector size must fit in PRDT byte count")
                - 1)
                | (1 << 31),
        );

        let command_fis = core::ptr::addr_of_mut!((*command_table).command_fis).cast::<u8>();
        core::ptr::write(command_fis.add(0), FIS_TYPE_REGISTER_HOST_TO_DEVICE);
        core::ptr::write(command_fis.add(1), FIS_COMMAND_FLAG);
        core::ptr::write(command_fis.add(2), ATA_COMMAND_READ_DMA_EXT);
        core::ptr::write(command_fis.add(7), ATA_DEVICE_LBA_MODE);
        core::ptr::write(command_fis.add(12), 1);
        core::ptr::write(command_fis.add(13), 0);
    }
}

fn dump_lba0(data_address: u64) {
    let data = data_address as *const u8;
    crate::serial_print!("[ahci ] LBA0:");
    for offset in 0..16 {
        // SAFETY: `data_address` points to a 512-byte DMA read buffer.
        let byte = unsafe { core::ptr::read_volatile(data.add(offset)) };
        crate::serial_print!(" {byte:02x}");
    }
    crate::serial_println!("");
}

fn split_address(address: u64) -> (u32, u32) {
    (
        u32::try_from(address & 0xffff_ffff).expect("low address bits must fit in u32"),
        u32::try_from(address >> 32).expect("high address bits must fit in u32"),
    )
}
