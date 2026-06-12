//! PCI bus scanning for storage controllers.

use crate::kernel::memory::address::PhysAddr;

const CONFIG_ENABLE_BIT: u32 = 1 << 31;
const MAX_BUS: u16 = 256;
const MAX_DEVICE: u8 = 32;
const MAX_FUNCTION: u8 = 8;
const VENDOR_ID_NONE: u16 = 0xffff;
const HEADER_TYPE_MULTIFUNCTION_BIT: u8 = 0x80;
const CLASS_MASS_STORAGE: u8 = 0x01;
const SUBCLASS_SATA: u8 = 0x06;
const COMMAND_OFFSET: u8 = 0x04;
const BASE_ADDRESS_REGISTER5_OFFSET: u8 = 0x24;
const BAR_MEMORY_MASK: u32 = 0xffff_fff0;
const COMMAND_MEMORY_SPACE_ENABLE: u32 = 1 << 1;
const COMMAND_BUS_MASTER_ENABLE: u32 = 1 << 2;
const VENDOR_DEVICE_OFFSET: u8 = 0x00;
const CLASS_REGISTER_OFFSET: u8 = 0x08;

/// Function pointer used to read one raw 32-bit PCI configuration address.
pub type PciConfigurationRead32 = fn(u32) -> u32;

/// Function pointer used to write one raw 32-bit PCI configuration address.
pub type PciConfigurationWrite32 = fn(u32, u32);

/// Provider for architecture-owned PCI configuration-space access.
#[derive(Clone, Copy)]
pub struct PciConfigurationAccess {
    read_config32: PciConfigurationRead32,
    write_config32: PciConfigurationWrite32,
}

impl PciConfigurationAccess {
    /// Create a PCI configuration-space access provider from architecture callbacks.
    pub fn new(
        read_config32: PciConfigurationRead32,
        write_config32: PciConfigurationWrite32,
    ) -> Self {
        Self {
            read_config32,
            write_config32,
        }
    }

    fn read_register(self, bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        (self.read_config32)(config_address(bus, device, function, offset))
    }

    fn write_register(self, bus: u8, device: u8, function: u8, offset: u8, value: u32) {
        (self.write_config32)(config_address(bus, device, function, offset), value);
    }
}

/// A discovered AHCI controller on the PCI bus.
pub struct AhciController {
    /// PCI bus number.
    pub bus: u8,
    /// PCI device number.
    pub device: u8,
    /// PCI function number.
    pub function: u8,
    /// Fifth base address register memory base address.
    pub base_address_register5: PhysAddr,
}

/// Find the first AHCI controller exposed through PCI config space.
pub fn find_advanced_host_controller_interface_controller(
    pci_configuration_access: PciConfigurationAccess,
) -> Option<AhciController> {
    crate::log_info!(
        "pci",
        "Starting legacy PCI scan: buses={} devices={} functions={}",
        MAX_BUS,
        MAX_DEVICE,
        MAX_FUNCTION
    );

    for bus in 0..MAX_BUS {
        let bus = u8::try_from(bus).expect("PCI bus range must fit in u8");
        for device in 0..MAX_DEVICE {
            if vendor_id(pci_configuration_access, bus, device, 0) == VENDOR_ID_NONE {
                continue;
            }

            log_device_function(pci_configuration_access, bus, device, 0);
            let function_count = if is_multifunction_device(pci_configuration_access, bus, device) {
                crate::log_debug!(
                    "pci",
                    "Multifunction device detected: bus={} dev={}",
                    bus,
                    device
                );
                MAX_FUNCTION
            } else {
                1
            };

            for function in 0..function_count {
                if vendor_id(pci_configuration_access, bus, device, function) == VENDOR_ID_NONE {
                    continue;
                }
                if function != 0 {
                    log_device_function(pci_configuration_access, bus, device, function);
                }

                if class_code(pci_configuration_access, bus, device, function) == CLASS_MASS_STORAGE
                    && subclass(pci_configuration_access, bus, device, function) == SUBCLASS_SATA
                {
                    crate::log_info!(
                        "pci",
                        "AHCI class match: bus={} device={} function={}",
                        bus,
                        device,
                        function
                    );
                    let command_before = enable_memory_bus_mastering(
                        pci_configuration_access,
                        bus,
                        device,
                        function,
                    );
                    let command_after = pci_configuration_access.read_register(
                        bus,
                        device,
                        function,
                        COMMAND_OFFSET,
                    );
                    let raw_base_address_register5 = pci_configuration_access.read_register(
                        bus,
                        device,
                        function,
                        BASE_ADDRESS_REGISTER5_OFFSET,
                    );
                    let base_address_register5 =
                        PhysAddr::new(u64::from(raw_base_address_register5 & BAR_MEMORY_MASK));
                    crate::log_debug!(
                        "pci",
                        "AHCI command register: before={:#010x} after={:#010x}",
                        command_before,
                        command_after
                    );
                    crate::log_debug!(
                        "pci",
                        "AHCI BAR5: raw={:#010x} memory_base={:#010x}",
                        raw_base_address_register5,
                        base_address_register5.as_u64()
                    );
                    crate::log_info!(
                        "pci",
                        "Found AHCI controller: bus={} device={} function={} bar5={:#010x}",
                        bus,
                        device,
                        function,
                        base_address_register5.as_u64()
                    );

                    return Some(AhciController {
                        bus,
                        device,
                        function,
                        base_address_register5,
                    });
                }
            }
        }
    }

    crate::log_warn!("pci", "Legacy PCI scan complete: AHCI controller not found");
    None
}

fn config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    CONFIG_ENABLE_BIT
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xfc)
}

fn vendor_id(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> u16 {
    (pci_configuration_access.read_register(bus, device, function, VENDOR_DEVICE_OFFSET) & 0xffff)
        as u16
}

fn device_id(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> u16 {
    ((pci_configuration_access.read_register(bus, device, function, VENDOR_DEVICE_OFFSET) >> 16)
        & 0xffff) as u16
}

fn header_type(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> u8 {
    ((pci_configuration_access.read_register(bus, device, function, 0x0c) >> 16) & 0xff) as u8
}

fn is_multifunction_device(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
) -> bool {
    header_type(pci_configuration_access, bus, device, 0) & HEADER_TYPE_MULTIFUNCTION_BIT != 0
}

fn class_code(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> u8 {
    ((pci_configuration_access.read_register(bus, device, function, CLASS_REGISTER_OFFSET) >> 24)
        & 0xff) as u8
}

fn subclass(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> u8 {
    ((pci_configuration_access.read_register(bus, device, function, CLASS_REGISTER_OFFSET) >> 16)
        & 0xff) as u8
}

fn programming_interface(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> u8 {
    ((pci_configuration_access.read_register(bus, device, function, CLASS_REGISTER_OFFSET) >> 8)
        & 0xff) as u8
}

fn log_device_function(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
    function: u8,
) {
    crate::log_debug!(
        "pci",
        "Device: bus={} device={} function={} vendor={:#06x} device_id={:#06x} class={:#04x} subclass={:#04x} interface={:#04x}",
        bus,
        device,
        function,
        vendor_id(pci_configuration_access, bus, device, function),
        device_id(pci_configuration_access, bus, device, function),
        class_code(pci_configuration_access, bus, device, function),
        subclass(pci_configuration_access, bus, device, function),
        programming_interface(pci_configuration_access, bus, device, function)
    );
}

fn enable_memory_bus_mastering(
    pci_configuration_access: PciConfigurationAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> u32 {
    let command = pci_configuration_access.read_register(bus, device, function, COMMAND_OFFSET);
    pci_configuration_access.write_register(
        bus,
        device,
        function,
        COMMAND_OFFSET,
        command | COMMAND_MEMORY_SPACE_ENABLE | COMMAND_BUS_MASTER_ENABLE,
    );
    command
}
