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

use crate::kernel::memory::frame_allocator::BumpFrameAllocator;

mod ahci;
mod gpt;
mod pci;

/// Discover and initialize supported storage controllers.
pub fn init(frame_allocator: &mut BumpFrameAllocator) {
    crate::log_info!("storage", "Initializing storage subsystem...");
    if let Some(controller) = pci::find_ahci_controller() {
        crate::log_info!(
            "storage",
            "AHCI controller selected: bus={} dev={} func={} bar5={:#010x}",
            controller.bus,
            controller.device,
            controller.function,
            controller.bar5
        );
        ahci::init(frame_allocator, controller.bar5);
        crate::log_info!("storage", "Storage subsystem initialization complete.");
    } else {
        crate::log_warn!("storage", "No supported storage controller found.");
    }
}
