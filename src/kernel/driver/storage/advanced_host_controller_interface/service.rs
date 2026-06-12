//! Persistent Advanced Host Controller Interface block-device service.

use spin::Mutex;

use crate::kernel::driver::storage::block_device::BlockDevice;
use crate::kernel::driver::storage::block_device::BlockDeviceResult;
use crate::kernel::memory::address::StorageDataAddress;

use super::block_device::AhciBlockDevice;

static PRIMARY_DEVICE: Mutex<Option<AhciBlockDevice>> = Mutex::new(None);

pub(super) fn register_primary_device(device: AhciBlockDevice) {
    crate::log_info!(
        "ahci",
        "Persistent block-device service registered: port={} data_buffer={:#018x} max_transfer={}",
        device.port_index(),
        device.data_address().as_u64(),
        device.maximum_transfer_sectors()
    );
    *PRIMARY_DEVICE.lock() = Some(device);
}

pub(in crate::kernel::driver::storage) fn get_primary_data_address() -> Option<StorageDataAddress> {
    PRIMARY_DEVICE
        .lock()
        .as_ref()
        .map(AhciBlockDevice::data_address)
}

pub(in crate::kernel::driver::storage) fn read_primary_blocks(
    logical_block_address: u64,
    sector_count: u16,
    data_address: StorageDataAddress,
) -> BlockDeviceResult<()> {
    let mut device = PRIMARY_DEVICE.lock();
    let Some(device) = device.as_mut() else {
        return Err(crate::kernel::driver::storage::block_device::BlockDeviceError::Unsupported);
    };
    device.read_logical_blocks(logical_block_address, sector_count, data_address)
}

/// Execute a read-only storage operation with the primary AHCI device locked.
pub(in crate::kernel::driver::storage) fn read_with_primary_device<T>(
    read: impl FnOnce(&mut AhciBlockDevice, StorageDataAddress) -> T,
) -> BlockDeviceResult<T> {
    let mut device = PRIMARY_DEVICE.lock();
    let Some(device) = device.as_mut() else {
        return Err(crate::kernel::driver::storage::block_device::BlockDeviceError::Unsupported);
    };
    let data_address = device.data_address();
    Ok(read(device, data_address))
}

#[allow(dead_code)]
pub(in crate::kernel::driver::storage) fn write_primary_blocks(
    logical_block_address: u64,
    sector_count: u16,
    data_address: StorageDataAddress,
) -> BlockDeviceResult<()> {
    let mut device = PRIMARY_DEVICE.lock();
    let Some(device) = device.as_mut() else {
        return Err(crate::kernel::driver::storage::block_device::BlockDeviceError::Unsupported);
    };
    device.write_logical_blocks(logical_block_address, sector_count, data_address)
}
