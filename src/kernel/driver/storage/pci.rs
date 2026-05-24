//! PCI configuration-space access and AHCI controller discovery.

const CONFIG_ENABLE_BIT: u32 = 1 << 31;
const MAX_BUS: u16 = 256;
const MAX_DEVICE: u8 = 32;
const MAX_FUNCTION: u8 = 8;
const VENDOR_ID_NONE: u16 = 0xffff;
const HEADER_TYPE_MULTIFUNCTION_BIT: u8 = 0x80;
const CLASS_MASS_STORAGE: u8 = 0x01;
const SUBCLASS_SATA: u8 = 0x06;
const BAR5_OFFSET: u8 = 0x24;
const BAR_MEMORY_MASK: u32 = 0xffff_fff0;

/// A discovered AHCI controller on the PCI bus.
pub struct AhciController {
    /// PCI bus number.
    pub bus: u8,
    /// PCI device number.
    pub device: u8,
    /// PCI function number.
    pub function: u8,
    /// AHCI BAR5 memory base address.
    pub bar5: u64,
}

/// Read one 32-bit PCI configuration register.
pub fn pci_config_read32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    crate::arch::x86_64::pci_configuration::read_config32(config_address(
        bus, device, function, offset,
    ))
}

/// Write one 32-bit PCI configuration register.
#[allow(dead_code)]
pub fn pci_config_write32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    crate::arch::x86_64::pci_configuration::write_config32(
        config_address(bus, device, function, offset),
        value,
    );
}

/// Find the first AHCI controller exposed through legacy PCI config space.
pub fn find_ahci_controller() -> Option<AhciController> {
    for bus in 0..MAX_BUS {
        let bus = u8::try_from(bus).expect("PCI bus range must fit in u8");
        for device in 0..MAX_DEVICE {
            if vendor_id(bus, device, 0) == VENDOR_ID_NONE {
                continue;
            }

            let function_count = if is_multifunction_device(bus, device) {
                MAX_FUNCTION
            } else {
                1
            };

            for function in 0..function_count {
                if vendor_id(bus, device, function) == VENDOR_ID_NONE {
                    continue;
                }

                if class_code(bus, device, function) == CLASS_MASS_STORAGE
                    && subclass(bus, device, function) == SUBCLASS_SATA
                {
                    let bar5 = u64::from(pci_config_read32(bus, device, function, BAR5_OFFSET));
                    let bar5 = bar5 & u64::from(BAR_MEMORY_MASK);
                    crate::serial_println!(
                        "[pci  ] Found AHCI controller: bus={} dev={} func={} bar5={:#010x}",
                        bus,
                        device,
                        function,
                        bar5
                    );

                    return Some(AhciController {
                        bus,
                        device,
                        function,
                        bar5,
                    });
                }
            }
        }
    }

    None
}

fn config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    CONFIG_ENABLE_BIT
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xfc)
}

fn vendor_id(bus: u8, device: u8, function: u8) -> u16 {
    (pci_config_read32(bus, device, function, 0x00) & 0xffff) as u16
}

fn header_type(bus: u8, device: u8, function: u8) -> u8 {
    ((pci_config_read32(bus, device, function, 0x0c) >> 16) & 0xff) as u8
}

fn is_multifunction_device(bus: u8, device: u8) -> bool {
    header_type(bus, device, 0) & HEADER_TYPE_MULTIFUNCTION_BIT != 0
}

fn class_code(bus: u8, device: u8, function: u8) -> u8 {
    ((pci_config_read32(bus, device, function, 0x08) >> 24) & 0xff) as u8
}

fn subclass(bus: u8, device: u8, function: u8) -> u8 {
    ((pci_config_read32(bus, device, function, 0x08) >> 16) & 0xff) as u8
}
