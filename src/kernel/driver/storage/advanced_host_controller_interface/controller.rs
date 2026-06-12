//! Advanced Host Controller Interface controller initialization.

use crate::kernel::memory::frame_allocator::BumpFrameAllocator;

use super::block_device::AhciBlockDevice;
use super::registers::MAX_PORTS;
use super::{command, service};
use super::{dma, host, port, probe};
use crate::kernel::driver::storage::block_device::SECTOR_BYTES;
use crate::kernel::driver::storage::{
    register_storage_device, StorageControllerKind, StorageDeviceId,
};

const SATA_SIGNATURE: u32 = 0x0000_0101;

/// Initialize an Advanced Host Controller Interface controller from its base address register 5 MMIO base.
pub fn init(frame_allocator: &mut BumpFrameAllocator, base_address_register5: u64) {
    let hba_memory = host::map_memory(frame_allocator, base_address_register5);
    host::enable_ahci(hba_memory);
    let ports_implemented = host::read_ports_implemented(hba_memory);

    for port_index in 0..MAX_PORTS {
        if !host::is_port_implemented(ports_implemented, port_index) {
            continue;
        }

        let hba_port = host::port_at(hba_memory, port_index);
        port::log_registers(port_index, hba_port);

        let signature = port::read_signature(hba_port);
        if signature == SATA_SIGNATURE {
            initialize_sata_port(frame_allocator, hba_port, port_index);
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

fn initialize_sata_port(
    frame_allocator: &mut BumpFrameAllocator,
    hba_port: *mut super::registers::HbaPort,
    port_index: usize,
) {
    crate::log_info!("ahci", "Port {}: SATA device detected", port_index);
    command::log_supported_transfer_directions(port_index);
    let Some(buffers) = dma::allocate(frame_allocator) else {
        crate::log_error!(
            "ahci",
            "Port {}: failed to allocate DMA buffers",
            port_index
        );
        return;
    };

    if !port::initialize_command_engine(hba_port, port_index, buffers) {
        return;
    }

    let mut block_device =
        AhciBlockDevice::new(hba_port, buffers, port_index, port::DEFAULT_COMPLETION_MODE);
    let maximum_transfer_sectors = block_device.maximum_transfer_sectors();
    probe::inspect_initial_storage(&mut block_device, buffers.data.as_u64());
    register_storage_device(
        StorageDeviceId {
            controller: StorageControllerKind::Ahci,
            controller_index: 0,
            port_index: u8::try_from(port_index).expect("AHCI port index must fit in u8"),
        },
        SECTOR_BYTES,
        maximum_transfer_sectors,
    );
    service::register_primary_device(block_device);
}
