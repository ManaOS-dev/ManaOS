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
mod pci;

/// Discover and initialize supported storage controllers.
pub fn init(frame_allocator: &mut BumpFrameAllocator) {
    if let Some(controller) = pci::find_ahci_controller() {
        crate::serial_println!(
            "[ahci ] Initializing controller at bus={} dev={} func={}",
            controller.bus,
            controller.device,
            controller.function
        );
        ahci::init(frame_allocator, controller.bar5);
    } else {
        crate::serial_println!("[pci  ] AHCI controller not found");
    }
}
