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
    crate::serial_println!("[storage] Initializing storage subsystem...");
    if let Some(controller) = pci::find_ahci_controller() {
        crate::serial_println!(
            "[storage] AHCI controller selected: bus={} dev={} func={} bar5={:#010x}",
            controller.bus,
            controller.device,
            controller.function,
            controller.bar5
        );
        ahci::init(frame_allocator, controller.bar5);
        crate::serial_println!("[storage] Storage subsystem initialization complete.");
    } else {
        crate::serial_println!("[storage] No supported storage controller found.");
    }
}
