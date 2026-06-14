//! Local APIC and IOAPIC MMIO register wrappers.

use super::{IOAPIC_REGISTER_SELECT_OFFSET, IOAPIC_REGISTER_WINDOW_OFFSET};
pub(super) struct LocalApicRegisters {
    base_address: usize,
}

impl LocalApicRegisters {
    pub(super) const fn new(base_address: usize) -> Self {
        Self { base_address }
    }

    pub(super) unsafe fn read(&self, register: usize) -> u32 {
        let register_pointer = self.register_pointer(register);
        // SAFETY: register_pointer points into mapped Local APIC MMIO space.
        // Volatile access is required for MMIO.
        unsafe { core::ptr::read_volatile(register_pointer) }
    }

    pub(super) unsafe fn write(&self, register: usize, value: u32) {
        let register_pointer = self.register_pointer(register);
        // SAFETY: register_pointer points into mapped Local APIC MMIO space.
        // Volatile access is required for MMIO.
        unsafe {
            core::ptr::write_volatile(register_pointer, value);
        }
    }

    fn register_pointer(&self, register: usize) -> *mut u32 {
        self.base_address
            .checked_add(register)
            .expect("Local APIC register address overflowed") as *mut u32
    }
}

pub(super) struct IoApicRegisters {
    base_address: usize,
}

impl IoApicRegisters {
    pub(super) fn new(physical_address: u64) -> Self {
        assert!(
            physical_address.is_multiple_of(4),
            "IOAPIC MMIO address must be 4-byte aligned"
        );
        Self {
            base_address: usize::try_from(physical_address)
                .expect("IOAPIC MMIO address must fit in usize"),
        }
    }

    pub(super) unsafe fn read(&self, register: u32) -> u32 {
        let register_select = self.register_select_pointer();
        let register_window = self.register_window_pointer();
        // SAFETY: register_select points into the mapped IOAPIC selector
        // register. Volatile access is required for MMIO.
        unsafe {
            core::ptr::write_volatile(register_select, register);
        }
        // SAFETY: register_window points into the mapped IOAPIC data window.
        // Volatile access is required for MMIO.
        unsafe { core::ptr::read_volatile(register_window) }
    }

    pub(super) unsafe fn write(&self, register: u32, value: u32) {
        let register_select = self.register_select_pointer();
        let register_window = self.register_window_pointer();
        // SAFETY: register_select points into the mapped IOAPIC selector
        // register. Volatile access is required for MMIO.
        unsafe {
            core::ptr::write_volatile(register_select, register);
        }
        // SAFETY: register_window points into the mapped IOAPIC data window.
        // Volatile access is required for MMIO.
        unsafe {
            core::ptr::write_volatile(register_window, value);
        }
    }

    fn register_select_pointer(&self) -> *mut u32 {
        self.base_address
            .checked_add(IOAPIC_REGISTER_SELECT_OFFSET)
            .expect("IOAPIC selector address overflowed") as *mut u32
    }

    fn register_window_pointer(&self) -> *mut u32 {
        self.base_address
            .checked_add(IOAPIC_REGISTER_WINDOW_OFFSET)
            .expect("IOAPIC window address overflowed") as *mut u32
    }
}
