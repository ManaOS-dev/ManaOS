//! # `kernel::driver::storage`
//!
//! ## Owns
//! - Storage subsystem public API surface
//! - Storage controller module composition
//! - Storage registry re-exports used by controller probes
//!
//! ## Does NOT own
//! - Architecture-specific port I/O (-> `arch`)
//! - Block filesystem parsing (-> `file_allocation_table`)
//! - Storage registry state and probing facade logic (-> `registry`)
//!
//! ## Public API
//! - [`init`] - Discover and initialize storage controllers
//! - [`PciConfigurationAccess`] - Provider for PCI configuration-space access
//! - [`get_detected_files`] - Return files detected from disk during probing

mod advanced_host_controller_interface;
mod block_device;
mod file_allocation_table;
mod guid_partition_table;
mod partition;
mod pci;
mod registry;

pub use pci::PciConfigurationAccess;
#[allow(unused_imports)]
pub use registry::{
    get_detected_files, get_primary_data_address, get_selected_partition, get_storage_devices,
    init, read_detected_file_range, read_primary_blocks, write_primary_blocks,
    StorageControllerKind, StorageDevice, StorageDeviceId, StorageFile, StoragePartition,
};
pub(in crate::kernel::driver::storage) use registry::{
    register_storage_device, set_detected_file, set_selected_partition,
};
