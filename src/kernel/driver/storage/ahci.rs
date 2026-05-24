//! AHCI controller discovery helpers.

use crate::kernel::memory::{frame_allocator::BumpFrameAllocator, paging};

const HBA_MEMORY_SIZE: u64 = 0x1100;
const MAX_PORTS: usize = 32;
const SATA_SIGNATURE: u32 = 0x0000_0101;

/// AHCI host bus adapter memory registers.
#[repr(C)]
pub struct HbaMemory {
    host_capability: u32,
    global_host_control: u32,
    interrupt_status: u32,
    ports_implemented: u32,
    version: u32,
    command_completion_coalescing_control: u32,
    command_completion_coalescing_ports: u32,
    enclosure_management_location: u32,
    enclosure_management_control: u32,
    host_capability_extended: u32,
    bios_os_handoff_control_status: u32,
    reserved: [u8; 0x74],
    vendor: [u8; 0x60],
    ports: [HbaPort; MAX_PORTS],
}

/// AHCI port register block.
#[repr(C)]
pub struct HbaPort {
    command_list_base: u32,
    command_list_base_upper: u32,
    fis_base: u32,
    fis_base_upper: u32,
    interrupt_status: u32,
    interrupt_enable: u32,
    command_and_status: u32,
    reserved0: u32,
    task_file_data: u32,
    signature: u32,
    sata_status: u32,
    sata_control: u32,
    sata_error: u32,
    sata_active: u32,
    command_issue: u32,
    sata_notification: u32,
    fis_based_switching_control: u32,
    reserved1: [u32; 11],
    vendor: [u32; 4],
}

/// Initialize an AHCI controller from its BAR5 MMIO base.
pub fn init(frame_allocator: &mut BumpFrameAllocator, bar5: u64) {
    // SAFETY: BAR5 is reported by a PCI mass-storage SATA controller and points
    // to the AHCI HBA MMIO register block.
    unsafe {
        paging::map_kernel_mmio_range(frame_allocator, bar5, HBA_MEMORY_SIZE);
    }

    let hba_memory = bar5 as *const HbaMemory;
    // SAFETY: The BAR5 MMIO range was mapped above, and `hba_memory` points to
    // the AHCI HBA register block.
    let ports_implemented =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).ports_implemented)) };
    crate::serial_println!("[ahci ] HBA ports implemented: {:#010x}", ports_implemented);

    for port_index in 0..MAX_PORTS {
        if ports_implemented & (1_u32 << port_index) == 0 {
            continue;
        }

        // SAFETY: `port_index` is below MAX_PORTS and this port is reported in
        // the HBA ports implemented bitmap.
        let signature = unsafe {
            let ports = core::ptr::addr_of!((*hba_memory).ports).cast::<HbaPort>();
            let port = ports.add(port_index);
            core::ptr::read_volatile(core::ptr::addr_of!((*port).signature))
        };
        if signature == SATA_SIGNATURE {
            crate::serial_println!("[ahci ] Port {}: SATA device detected", port_index);
        } else {
            crate::serial_println!(
                "[ahci ] Port {}: non-SATA signature {:#010x}",
                port_index,
                signature
            );
        }
    }
}
