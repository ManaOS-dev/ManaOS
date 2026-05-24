//! # `arch::x86_64::pci_configuration`
//!
//! ## Owns
//! - Legacy PCI configuration port access
//!
//! ## Does NOT own
//! - PCI bus enumeration policy (-> `kernel::driver::storage::pci`)
//! - Device-specific driver initialization
//!
//! ## Public API
//! - [`read_config32`] - Read one 32-bit PCI configuration register
//! - [`write_config32`] - Write one 32-bit PCI configuration register

use x86_64::instructions::port::Port;

const PCI_CONFIG_ADDRESS_PORT: u16 = 0x0cf8;
const PCI_CONFIG_DATA_PORT: u16 = 0x0cfc;

/// Read one 32-bit value from the legacy PCI configuration data port.
pub fn read_config32(address: u32) -> u32 {
    let mut address_port = Port::<u32>::new(PCI_CONFIG_ADDRESS_PORT);
    let mut data_port = Port::<u32>::new(PCI_CONFIG_DATA_PORT);

    // SAFETY: Ports 0xCF8/0xCFC are the architectural legacy PCI
    // configuration address/data ports on x86-compatible systems.
    unsafe {
        address_port.write(address);
        data_port.read()
    }
}

/// Write one 32-bit value through the legacy PCI configuration data port.
pub fn write_config32(address: u32, value: u32) {
    let mut address_port = Port::<u32>::new(PCI_CONFIG_ADDRESS_PORT);
    let mut data_port = Port::<u32>::new(PCI_CONFIG_DATA_PORT);

    // SAFETY: Ports 0xCF8/0xCFC are the architectural legacy PCI
    // configuration address/data ports on x86-compatible systems.
    unsafe {
        address_port.write(address);
        data_port.write(value);
    }
}
