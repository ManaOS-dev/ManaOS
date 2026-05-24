//! Memory-mapped register and command table layouts.

/// Maximum number of controller ports represented in the host bus adapter register block.
pub(super) const MAX_PORTS: usize = 32;

/// Advanced Host Controller Interface host bus adapter memory registers.
#[repr(C)]
pub(super) struct HbaMemory {
    pub(super) host_capability: u32,
    pub(super) global_host_control: u32,
    pub(super) interrupt_status: u32,
    pub(super) ports_implemented: u32,
    pub(super) version: u32,
    pub(super) command_completion_coalescing_control: u32,
    pub(super) command_completion_coalescing_ports: u32,
    pub(super) enclosure_management_location: u32,
    pub(super) enclosure_management_control: u32,
    pub(super) host_capability_extended: u32,
    pub(super) bios_os_handoff_control_status: u32,
    pub(super) reserved: [u8; 0x74],
    pub(super) vendor: [u8; 0x60],
    pub(super) ports: [HbaPort; MAX_PORTS],
}

/// Advanced Host Controller Interface port register block.
#[repr(C)]
pub(super) struct HbaPort {
    pub(super) command_list_base: u32,
    pub(super) command_list_base_upper: u32,
    pub(super) fis_base: u32,
    pub(super) fis_base_upper: u32,
    pub(super) interrupt_status: u32,
    pub(super) interrupt_enable: u32,
    pub(super) command_and_status: u32,
    pub(super) reserved0: u32,
    pub(super) task_file_data: u32,
    pub(super) signature: u32,
    pub(super) sata_status: u32,
    pub(super) sata_control: u32,
    pub(super) sata_error: u32,
    pub(super) sata_active: u32,
    pub(super) command_issue: u32,
    pub(super) sata_notification: u32,
    pub(super) fis_based_switching_control: u32,
    pub(super) reserved1: [u32; 11],
    pub(super) vendor: [u32; 4],
}

#[repr(C)]
pub(super) struct HbaCommandHeader {
    pub(super) flags: u16,
    pub(super) prdt_length: u16,
    pub(super) prd_byte_count: u32,
    pub(super) command_table_base: u32,
    pub(super) command_table_base_upper: u32,
    pub(super) reserved: [u32; 4],
}

#[repr(C)]
pub(super) struct HbaPrdtEntry {
    pub(super) data_base: u32,
    pub(super) data_base_upper: u32,
    pub(super) reserved: u32,
    pub(super) byte_count_and_interrupt: u32,
}

#[repr(C)]
pub(super) struct HbaCommandTable {
    pub(super) command_fis: [u8; 64],
    pub(super) atapi_command: [u8; 16],
    pub(super) reserved: [u8; 48],
    pub(super) prdt_entries: [HbaPrdtEntry; 1],
}
