//! Advanced Host Controller Interface host bus adapter MMIO setup.

use crate::kernel::memory::{frame_allocator::BumpFrameAllocator, paging};

use super::registers::{HbaMemory, HbaPort, MAX_PORTS};

const HOST_BUS_ADAPTER_MEMORY_SIZE: u64 = 0x1100;
const GLOBAL_HOST_CONTROL_AHCI_ENABLE: u32 = 1 << 31;

pub(super) fn map_memory(
    frame_allocator: &mut BumpFrameAllocator,
    base_address_register5: u64,
) -> *mut HbaMemory {
    crate::log_debug!(
        "ahci",
        "Mapping HBA MMIO: base={:#010x} size={:#x}",
        base_address_register5,
        HOST_BUS_ADAPTER_MEMORY_SIZE
    );
    // SAFETY: base address register 5 is reported by a PCI mass-storage SATA
    // controller and points to the Advanced Host Controller Interface host bus
    // adapter MMIO register block.
    unsafe {
        paging::map_kernel_mmio_range(
            frame_allocator,
            base_address_register5,
            HOST_BUS_ADAPTER_MEMORY_SIZE,
        );
    }
    crate::log_debug!("ahci", "HBA MMIO mapping complete.");

    base_address_register5 as *mut HbaMemory
}

pub(super) fn enable_ahci(hba_memory: *mut HbaMemory) {
    // SAFETY: `hba_memory` points to the mapped Advanced Host Controller
    // Interface host bus adapter register block.
    let global_host_control =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).global_host_control)) };
    crate::log_debug!(
        "ahci",
        "Global host control before enable: {:#010x}",
        global_host_control
    );
    // SAFETY: `hba_memory` points to the mapped Advanced Host Controller
    // Interface host bus adapter register block.
    unsafe {
        core::ptr::write_volatile(
            core::ptr::addr_of_mut!((*hba_memory).global_host_control),
            global_host_control | GLOBAL_HOST_CONTROL_AHCI_ENABLE,
        );
    }
    // SAFETY: `hba_memory` points to the mapped Advanced Host Controller
    // Interface host bus adapter register block.
    let enabled_global_host_control =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).global_host_control)) };
    crate::log_debug!(
        "ahci",
        "Global host control after enable: {:#010x}",
        enabled_global_host_control
    );
}

pub(super) fn read_ports_implemented(hba_memory: *mut HbaMemory) -> u32 {
    // SAFETY: The base address register 5 MMIO range was mapped, and
    // `hba_memory` points to the Advanced Host Controller Interface host bus
    // adapter register block.
    let ports_implemented =
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!((*hba_memory).ports_implemented)) };
    crate::log_info!("ahci", "HBA ports implemented: {:#010x}", ports_implemented);
    ports_implemented
}

pub(super) fn is_port_implemented(ports_implemented: u32, port_index: usize) -> bool {
    ports_implemented & (1_u32 << port_index) != 0
}

pub(super) fn port_at(hba_memory: *mut HbaMemory, port_index: usize) -> *mut HbaPort {
    debug_assert!(port_index < MAX_PORTS);
    // SAFETY: `port_index` is below MAX_PORTS and the caller uses this helper
    // for ports reported in the host bus adapter ports implemented bitmap.
    unsafe {
        let ports = core::ptr::addr_of_mut!((*hba_memory).ports).cast::<HbaPort>();
        ports.add(port_index)
    }
}
