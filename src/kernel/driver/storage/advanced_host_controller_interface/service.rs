//! Persistent Advanced Host Controller Interface block-device service.

use spin::Mutex;

use crate::kernel::driver::storage::block_device::BlockDevice;
use crate::kernel::driver::storage::block_device::BlockDeviceResult;

use super::block_device::AhciBlockDevice;

static PRIMARY_DEVICE: Mutex<Option<AhciBlockDevice>> = Mutex::new(None);

pub(super) fn register_primary_device(device: AhciBlockDevice) {
    crate::log_info!(
        "ahci",
        "Persistent block-device service registered: port={} data_buffer={:#018x} max_transfer={}",
        device.port_index(),
        device.data_address(),
        device.maximum_transfer_sectors()
    );
    *PRIMARY_DEVICE.lock() = Some(device);
}

pub(in crate::kernel::driver::storage) fn get_primary_data_address() -> Option<u64> {
    PRIMARY_DEVICE
        .lock()
        .as_ref()
        .map(AhciBlockDevice::data_address)
}

pub(in crate::kernel::driver::storage) fn read_primary_blocks(
    logical_block_address: u64,
    sector_count: u16,
    data_address: u64,
) -> BlockDeviceResult<()> {
    let mut device = PRIMARY_DEVICE.lock();
    let Some(device) = device.as_mut() else {
        return Err(crate::kernel::driver::storage::block_device::BlockDeviceError::Unsupported);
    };
    device.read_logical_blocks(logical_block_address, sector_count, data_address)
}

#[allow(dead_code)]
pub(in crate::kernel::driver::storage) fn write_primary_blocks(
    logical_block_address: u64,
    sector_count: u16,
    data_address: u64,
) -> BlockDeviceResult<()> {
    let mut device = PRIMARY_DEVICE.lock();
    let Some(device) = device.as_mut() else {
        return Err(crate::kernel::driver::storage::block_device::BlockDeviceError::Unsupported);
    };
    device.write_logical_blocks(logical_block_address, sector_count, data_address)
}
