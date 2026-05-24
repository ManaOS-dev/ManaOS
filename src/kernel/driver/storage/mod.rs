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

use crate::kernel::memory::frame_allocator::BumpFrameAllocator;
use spin::Mutex;

mod advanced_host_controller_interface;
mod block_device;
mod guid_partition_table;
mod pci;

pub use pci::PciConfigurationAccess;

static SELECTED_PARTITION: Mutex<Option<StoragePartition>> = Mutex::new(None);

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

/// Discover and initialize supported storage controllers.
pub fn init(
    frame_allocator: &mut BumpFrameAllocator,
    pci_configuration_access: PciConfigurationAccess,
) {
    crate::log_info!("storage", "Initializing storage subsystem...");
    if let Some(controller) =
        pci::find_advanced_host_controller_interface_controller(pci_configuration_access)
    {
        crate::log_info!(
            "storage",
            "Advanced Host Controller Interface storage controller selected: bus={} device={} function={} base_address_register5={:#010x}",
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

pub(super) fn set_selected_partition(partition: guid_partition_table::GuidPartitionTablePartition) {
    *SELECTED_PARTITION.lock() = Some(StoragePartition {
        index: partition.index,
        first_lba: partition.first_lba,
        last_lba: partition.last_lba,
        name: partition.name,
        name_length: partition.name_length,
    });
}
