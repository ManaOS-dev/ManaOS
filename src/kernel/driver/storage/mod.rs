//! # `kernel::driver::storage`
//!
//! ## Owns
//! - Storage controller discovery
//! - Storage driver initialization entry point
//!
//! ## Does NOT own
//! - Architecture-specific port I/O (-> `arch`)
//! - Block filesystem parsing
//!
//! ## Public API
//! - [`init`] - Discover and initialize storage controllers
//! - [`PciConfigurationAccess`] - Provider for PCI configuration-space access
//! - [`get_detected_file`] - Return the first file loaded from disk during probing

use crate::kernel::memory::frame_allocator::BumpFrameAllocator;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use spin::Mutex;

mod advanced_host_controller_interface;
mod block_device;
mod file_allocation_table;
mod guid_partition_table;
mod partition;
mod pci;

pub use pci::PciConfigurationAccess;

static SELECTED_PARTITION: Mutex<Option<StoragePartition>> = Mutex::new(None);
static DETECTED_FILE: Mutex<Option<StorageFile>> = Mutex::new(None);
static STORAGE_DEVICES: Mutex<Vec<StorageDevice>> = Mutex::new(Vec::new());

/// Stable storage-device identifier.
#[derive(Clone, Copy)]
pub struct StorageDeviceId {
    /// Controller driver family.
    pub controller: StorageControllerKind,
    /// Zero-based controller index within the driver family.
    pub controller_index: u8,
    /// Zero-based port index within the controller.
    pub port_index: u8,
}

impl fmt::Display for StorageDeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}{}p{}",
            self.controller, self.controller_index, self.port_index
        )
    }
}

/// Storage controller driver family.
#[derive(Clone, Copy)]
pub enum StorageControllerKind {
    /// Advanced Host Controller Interface SATA controller.
    Ahci,
}

impl fmt::Display for StorageControllerKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ahci => write!(formatter, "ahci"),
        }
    }
}

/// Registered block-device metadata.
#[derive(Clone)]
pub struct StorageDevice {
    /// Stable device identifier.
    pub id: String,
    /// Logical sector size in bytes.
    pub sector_size: usize,
    /// Maximum sectors transferred by one command.
    pub maximum_transfer_sectors: u16,
}

/// Storage partition selected by the kernel storage probe.
#[derive(Clone, Copy)]
pub struct StoragePartition {
    /// Index in the GUID partition table partition entry array.
    pub index: u32,
    /// First LBA owned by this partition.
    pub first_lba: u64,
    /// Last LBA owned by this partition.
    pub last_lba: u64,
    /// ASCII fallback partition name bytes.
    pub name: [u8; 36],
    /// Number of valid bytes in [`Self::name`].
    pub name_length: usize,
}

impl StoragePartition {
    /// Return the selected partition name as ASCII fallback text.
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_length])
            .expect("storage partition names are stored as ASCII fallback bytes")
    }
}

/// File content loaded from storage during early probing.
#[derive(Clone)]
pub struct StorageFile {
    /// Absolute path where the file should be mounted.
    pub mount_path: String,
    /// File bytes read from the storage device.
    pub contents: Vec<u8>,
}

/// Discover and initialize supported storage controllers.
pub fn init(
    frame_allocator: &mut BumpFrameAllocator,
    pci_configuration_access: PciConfigurationAccess,
) {
    STORAGE_DEVICES.lock().clear();
    crate::log_info!("storage", "Initializing storage subsystem...");
    if let Some(controller) =
        pci::find_advanced_host_controller_interface_controller(pci_configuration_access)
    {
        crate::log_info!(
            "ahci",
            "Selected controller: bus={} device={} function={} bar5={:#010x}",
            controller.bus,
            controller.device,
            controller.function,
            controller.base_address_register5
        );
        advanced_host_controller_interface::init(
            frame_allocator,
            controller.base_address_register5,
        );
        crate::log_info!("storage", "Storage subsystem initialization complete.");
    } else {
        crate::log_warn!("storage", "No supported storage controller found.");
    }
}

/// Return the partition selected by the storage probe.
pub fn get_selected_partition() -> Option<StoragePartition> {
    *SELECTED_PARTITION.lock()
}

/// Return the first file loaded from disk during storage probing.
pub fn get_detected_file() -> Option<StorageFile> {
    DETECTED_FILE.lock().clone()
}

/// Return registered storage devices.
pub fn get_storage_devices() -> Vec<StorageDevice> {
    STORAGE_DEVICES.lock().clone()
}

/// Read sectors through the primary persistent block device.
pub fn read_primary_blocks(
    logical_block_address: u64,
    sector_count: u16,
    data_address: u64,
) -> bool {
    match advanced_host_controller_interface::read_primary_blocks(
        logical_block_address,
        sector_count,
        data_address,
    ) {
        Ok(()) => true,
        Err(error) => {
            crate::log_warn!("storage", "primary read failed: error={error:?}");
            false
        }
    }
}

/// Return the primary block device DMA data buffer address.
pub fn get_primary_data_address() -> Option<u64> {
    advanced_host_controller_interface::get_primary_data_address()
}

/// Write sectors through the primary persistent block device.
#[allow(dead_code)]
pub fn write_primary_blocks(
    logical_block_address: u64,
    sector_count: u16,
    data_address: u64,
) -> bool {
    match advanced_host_controller_interface::write_primary_blocks(
        logical_block_address,
        sector_count,
        data_address,
    ) {
        Ok(()) => true,
        Err(error) => {
            crate::log_warn!("storage", "primary write failed: error={error:?}");
            false
        }
    }
}

pub(super) fn set_selected_partition(partition: guid_partition_table::GuidPartitionTablePartition) {
    *SELECTED_PARTITION.lock() = Some(StoragePartition {
        index: partition.index,
        first_lba: partition.first_lba,
        last_lba: partition.last_lba,
        name: partition.name,
        name_length: partition.name_length,
    });
}

pub(super) fn set_detected_file(file: StorageFile) {
    *DETECTED_FILE.lock() = Some(file);
}

pub(super) fn register_storage_device(
    id: StorageDeviceId,
    sector_size: usize,
    maximum_transfer_sectors: u16,
) {
    let device = StorageDevice {
        id: format!("{id}"),
        sector_size,
        maximum_transfer_sectors,
    };
    crate::log_info!(
        "storage",
        "Registered block device: id={} sector_size={} max_transfer={}",
        device.id,
        device.sector_size,
        device.maximum_transfer_sectors
    );
    STORAGE_DEVICES.lock().push(device);
}
