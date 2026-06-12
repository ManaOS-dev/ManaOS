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
//! - [`get_detected_files`] - Return files detected from disk during probing

use crate::kernel::memory::address::StorageDataAddress;
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
static DETECTED_FILES: Mutex<Vec<StorageFile>> = Mutex::new(Vec::new());
static DETECTED_FAT32_FILES: Mutex<Vec<DetectedFat32File>> = Mutex::new(Vec::new());
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

/// File discovered from storage during early probing.
#[derive(Clone)]
pub struct StorageFile {
    /// Absolute path where the file should be mounted.
    pub mount_path: String,
    /// File size in bytes.
    pub size: usize,
    /// Backend context index used when reading this file.
    pub backend_index: usize,
}

#[derive(Clone, Copy)]
struct DetectedFat32File {
    partition: guid_partition_table::GuidPartitionTablePartition,
    volume: file_allocation_table::FileAllocationTable32Volume,
    entry: file_allocation_table::FileAllocationTable32DirectoryEntry,
}

/// Discover and initialize supported storage controllers.
pub fn init(
    frame_allocator: &mut BumpFrameAllocator,
    pci_configuration_access: PciConfigurationAccess,
) {
    STORAGE_DEVICES.lock().clear();
    DETECTED_FILES.lock().clear();
    DETECTED_FAT32_FILES.lock().clear();
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
            controller.base_address_register5.as_u64()
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

/// Return files loaded from disk during storage probing.
pub fn get_detected_files() -> Vec<StorageFile> {
    DETECTED_FILES.lock().clone()
}

/// Return registered storage devices.
pub fn get_storage_devices() -> Vec<StorageDevice> {
    STORAGE_DEVICES.lock().clone()
}

/// Read sectors through the primary persistent block device.
pub fn read_primary_blocks(
    logical_block_address: u64,
    sector_count: u16,
    data_address: StorageDataAddress,
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
pub fn get_primary_data_address() -> Option<StorageDataAddress> {
    advanced_host_controller_interface::get_primary_data_address()
}

/// Read from the detected FAT32 file backend into `buffer`.
pub fn read_detected_file_range(
    backend_index: usize,
    offset: usize,
    buffer: &mut [u8],
) -> Option<usize> {
    let detected_files = DETECTED_FAT32_FILES.lock();
    let detected_file = detected_files.get(backend_index).copied()?;
    advanced_host_controller_interface::read_with_primary_device(|block_device, data_address| {
        let mut partition_device = partition::PartitionBlockDevice::new(
            block_device,
            detected_file.partition.first_lba,
            detected_file.partition.last_lba,
        );
        file_allocation_table::read_file_range(
            &mut partition_device,
            detected_file.volume,
            detected_file.entry,
            data_address,
            offset,
            buffer,
        )
    })
    .ok()
    .flatten()
}

/// Write sectors through the primary persistent block device.
#[allow(dead_code)]
pub fn write_primary_blocks(
    logical_block_address: u64,
    sector_count: u16,
    data_address: StorageDataAddress,
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

/// Record a detected FAT32 file and expose it as a read-only backend candidate.
pub(in crate::kernel::driver::storage) fn set_detected_file(
    partition: guid_partition_table::GuidPartitionTablePartition,
    volume: file_allocation_table::FileAllocationTable32Volume,
    entry: file_allocation_table::FileAllocationTable32DirectoryEntry,
    mount_path: String,
) {
    let size = usize::try_from(entry.file_size()).expect("FAT32 file size must fit in usize");
    let backend_index = {
        let mut detected_fat32_files = DETECTED_FAT32_FILES.lock();
        let backend_index = detected_fat32_files.len();
        detected_fat32_files.push(DetectedFat32File {
            partition,
            volume,
            entry,
        });
        backend_index
    };
    DETECTED_FILES.lock().push(StorageFile {
        mount_path,
        size,
        backend_index,
    });
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
