//! Interrupt controller status and diagnostics records.

use super::{
    has_ioapic_routing, has_local_apic, ApicRoutingConfiguration, IoApicConfiguration,
    IoApicRedirectionEntry, IoApicRedirectionPlan, LegacyIrqRoute, LocalApicConfiguration,
    APIC_PROVIDER_CONFIGURED_FLAG, APIC_PROVIDER_LOCAL_APIC_SUPPORTED_FLAG,
    APIC_PROVIDER_ROUTING_ACTIVE_FLAG, APIC_PROVIDER_TRUNCATED_FLAG,
    IOAPIC_ACTIVATION_ALL_UNMASKED_FLAG, IOAPIC_ACTIVATION_LEGACY_PIC_MASKED_FLAG,
    IOAPIC_ACTIVATION_LOCAL_APIC_SOFTWARE_ENABLED_FLAG, IOAPIC_ACTIVATION_READBACK_MATCHED_FLAG,
    IOAPIC_ACTIVATION_ROUTING_ACTIVE_FLAG, IOAPIC_REDIRECTION_LOW_READBACK_MASK,
    IOAPIC_REDIRECTION_MASKED_BIT, IOAPIC_STAGING_ALL_MASKED_FLAG,
    IOAPIC_STAGING_READBACK_MATCHED_FLAG, IOAPIC_TIMER_MASK_MASKED_FLAG,
    IOAPIC_TIMER_MASK_READBACK_MATCHED_FLAG, IOAPIC_TIMER_MASK_ROUTING_ACTIVE_FLAG,
    LEGACY_KEYBOARD_IRQ, LEGACY_MOUSE_IRQ, LEGACY_PIC_FALLBACK_ENABLED_FLAG,
    LEGACY_PIC_INITIALIZED_FLAG, LEGACY_PIC_MASKED_FOR_APIC_ROUTING_FLAG, LEGACY_TIMER_IRQ,
    LOCAL_APIC_EOI_PROVIDER_CONFIGURED_FLAG, LOCAL_APIC_EOI_PROVIDER_ROUTING_ACTIVE_FLAG,
    LOCAL_APIC_EOI_PROVIDER_SOFTWARE_ENABLED_FLAG, LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_MASK,
    LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_NUMBER,
};
/// APIC routing provider status used for boot diagnostics.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ApicRoutingProviderStatus {
    flags: u8,
    local_apic: LocalApicConfiguration,
    ioapic: IoApicConfiguration,
    redirection_plan: IoApicRedirectionPlan,
    legacy_irq_route_count: usize,
    legacy_irq0_global_system_interrupt: u32,
    legacy_irq0_flags: u16,
    legacy_irq1_global_system_interrupt: u32,
    legacy_irq12_global_system_interrupt: u32,
}

impl ApicRoutingProviderStatus {
    pub(in crate::arch::x86_64::interrupt_controller) const fn unavailable() -> Self {
        Self {
            flags: 0,
            local_apic: LocalApicConfiguration::EMPTY,
            ioapic: IoApicConfiguration::EMPTY,
            redirection_plan: IoApicRedirectionPlan::new(),
            legacy_irq_route_count: 0,
            legacy_irq0_global_system_interrupt: 0,
            legacy_irq0_flags: 0,
            legacy_irq1_global_system_interrupt: 1,
            legacy_irq12_global_system_interrupt: 12,
        }
    }

    pub(in crate::arch::x86_64::interrupt_controller) fn from_configuration(
        configuration: &ApicRoutingConfiguration,
    ) -> Self {
        let legacy_irq0_route = configuration.legacy_irq_route_for_irq(0);
        let mut flags = APIC_PROVIDER_CONFIGURED_FLAG;
        if has_ioapic_routing() {
            flags |= APIC_PROVIDER_ROUTING_ACTIVE_FLAG;
        }
        if has_local_apic() {
            flags |= APIC_PROVIDER_LOCAL_APIC_SUPPORTED_FLAG;
        }
        if configuration.is_truncated() {
            flags |= APIC_PROVIDER_TRUNCATED_FLAG;
        }
        Self {
            flags,
            local_apic: configuration.local_apic(),
            ioapic: configuration.ioapic(),
            redirection_plan: IoApicRedirectionPlan::from_configuration(configuration),
            legacy_irq_route_count: configuration.legacy_irq_route_count(),
            legacy_irq0_global_system_interrupt: configuration
                .global_system_interrupt_for_legacy_irq(0),
            legacy_irq0_flags: legacy_irq0_route.map_or(0, LegacyIrqRoute::flags),
            legacy_irq1_global_system_interrupt: configuration
                .global_system_interrupt_for_legacy_irq(1),
            legacy_irq12_global_system_interrupt: configuration
                .global_system_interrupt_for_legacy_irq(12),
        }
    }

    /// Return whether APIC routing provider data has been configured.
    pub const fn is_configured(self) -> bool {
        self.flags & APIC_PROVIDER_CONFIGURED_FLAG != 0
    }

    /// Return whether IOAPIC routing is active for hardware interrupts.
    pub const fn is_routing_active(self) -> bool {
        self.flags & APIC_PROVIDER_ROUTING_ACTIVE_FLAG != 0
    }

    /// Return whether this CPU reports Local APIC support.
    pub const fn has_local_apic_support(self) -> bool {
        self.flags & APIC_PROVIDER_LOCAL_APIC_SUPPORTED_FLAG != 0
    }

    /// Return the configured Local APIC record.
    pub const fn local_apic(self) -> LocalApicConfiguration {
        self.local_apic
    }

    /// Return the configured IOAPIC record.
    pub const fn ioapic(self) -> IoApicConfiguration {
        self.ioapic
    }

    /// Return the planned IOAPIC redirection entries.
    pub const fn redirection_plan(self) -> IoApicRedirectionPlan {
        self.redirection_plan
    }

    /// Return the number of retained legacy IRQ override routes.
    pub const fn legacy_irq_route_count(self) -> usize {
        self.legacy_irq_route_count
    }

    /// Return the resolved global system interrupt for legacy IRQ0.
    pub const fn legacy_irq0_global_system_interrupt(self) -> u32 {
        self.legacy_irq0_global_system_interrupt
    }

    /// Return the raw ACPI flags for legacy IRQ0 when overridden.
    pub const fn legacy_irq0_flags(self) -> u16 {
        self.legacy_irq0_flags
    }

    /// Return the resolved global system interrupt for legacy IRQ1.
    pub const fn legacy_irq1_global_system_interrupt(self) -> u32 {
        self.legacy_irq1_global_system_interrupt
    }

    /// Return the resolved global system interrupt for legacy IRQ12.
    pub const fn legacy_irq12_global_system_interrupt(self) -> u32 {
        self.legacy_irq12_global_system_interrupt
    }

    /// Return whether provider route records exceeded retained capacity.
    pub const fn is_truncated(self) -> bool {
        self.flags & APIC_PROVIDER_TRUNCATED_FLAG != 0
    }
}

/// Result of staging masked IOAPIC redirection entries.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IoApicRedirectionStagingStatus {
    flags: u8,
    version: u32,
    maximum_redirection_entry: u32,
    planned_entry_count: usize,
    staged_entry_count: usize,
    out_of_range_entry_count: usize,
    timer_low_readback: u32,
    timer_high_readback: u32,
    keyboard_low_readback: u32,
    keyboard_high_readback: u32,
    mouse_low_readback: u32,
    mouse_high_readback: u32,
}

impl IoApicRedirectionStagingStatus {
    pub(in crate::arch::x86_64::interrupt_controller) const fn new(
        version: u32,
        maximum_redirection_entry: u32,
        planned_entry_count: usize,
    ) -> Self {
        Self {
            flags: IOAPIC_STAGING_READBACK_MATCHED_FLAG | IOAPIC_STAGING_ALL_MASKED_FLAG,
            version,
            maximum_redirection_entry,
            planned_entry_count,
            staged_entry_count: 0,
            out_of_range_entry_count: 0,
            timer_low_readback: 0,
            timer_high_readback: 0,
            keyboard_low_readback: 0,
            keyboard_high_readback: 0,
            mouse_low_readback: 0,
            mouse_high_readback: 0,
        }
    }

    /// Return the raw IOAPIC version register readback.
    pub const fn version(self) -> u32 {
        self.version
    }

    /// Return the maximum supported redirection table index.
    pub const fn maximum_redirection_entry(self) -> u32 {
        self.maximum_redirection_entry
    }

    /// Return the number of redirection entries from the current plan.
    pub const fn planned_entry_count(self) -> usize {
        self.planned_entry_count
    }

    /// Return the number of redirection entries written and read back.
    pub const fn staged_entry_count(self) -> usize {
        self.staged_entry_count
    }

    /// Return the number of planned entries outside the IOAPIC table range.
    pub const fn out_of_range_entry_count(self) -> usize {
        self.out_of_range_entry_count
    }

    /// Return whether all staged entries matched their masked readback values.
    pub const fn readback_matches(self) -> bool {
        self.flags & IOAPIC_STAGING_READBACK_MATCHED_FLAG != 0
    }

    /// Return whether all staged entries remained masked after readback.
    pub const fn all_entries_masked(self) -> bool {
        self.flags & IOAPIC_STAGING_ALL_MASKED_FLAG != 0
    }

    /// Return the timer low redirection dword readback.
    pub const fn timer_low_readback(self) -> u32 {
        self.timer_low_readback
    }

    /// Return the timer high redirection dword readback.
    pub const fn timer_high_readback(self) -> u32 {
        self.timer_high_readback
    }

    /// Return the keyboard low redirection dword readback.
    pub const fn keyboard_low_readback(self) -> u32 {
        self.keyboard_low_readback
    }

    /// Return the keyboard high redirection dword readback.
    pub const fn keyboard_high_readback(self) -> u32 {
        self.keyboard_high_readback
    }

    /// Return the mouse low redirection dword readback.
    pub const fn mouse_low_readback(self) -> u32 {
        self.mouse_low_readback
    }

    /// Return the mouse high redirection dword readback.
    pub const fn mouse_high_readback(self) -> u32 {
        self.mouse_high_readback
    }

    pub(in crate::arch::x86_64::interrupt_controller) fn record_staged_entry(
        &mut self,
        entry: IoApicRedirectionEntry,
        low_value: u32,
        high_value: u32,
        low_readback: u32,
        high_readback: u32,
    ) {
        self.staged_entry_count += 1;
        if (low_readback & IOAPIC_REDIRECTION_LOW_READBACK_MASK) != low_value
            || high_readback != high_value
        {
            self.flags &= !IOAPIC_STAGING_READBACK_MATCHED_FLAG;
        }
        if low_readback & IOAPIC_REDIRECTION_MASKED_BIT == 0 {
            self.flags &= !IOAPIC_STAGING_ALL_MASKED_FLAG;
        }
        match entry.legacy_irq() {
            LEGACY_TIMER_IRQ => {
                self.timer_low_readback = low_readback;
                self.timer_high_readback = high_readback;
            }
            LEGACY_KEYBOARD_IRQ => {
                self.keyboard_low_readback = low_readback;
                self.keyboard_high_readback = high_readback;
            }
            LEGACY_MOUSE_IRQ => {
                self.mouse_low_readback = low_readback;
                self.mouse_high_readback = high_readback;
            }
            _ => {}
        }
    }

    pub(in crate::arch::x86_64::interrupt_controller) fn record_out_of_range_entry(&mut self) {
        self.out_of_range_entry_count += 1;
        self.flags &= !IOAPIC_STAGING_READBACK_MATCHED_FLAG;
    }
}

/// Local APIC EOI provider diagnostics.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LocalApicEoiProviderStatus {
    flags: u8,
    physical_address: u64,
    apic_id: u32,
    version: u32,
    maximum_lvt_entry: u32,
    spurious_interrupt_vector: u32,
}

impl LocalApicEoiProviderStatus {
    pub(in crate::arch::x86_64::interrupt_controller) const fn new(
        flags: u8,
        physical_address: u64,
        apic_id: u32,
        version: u32,
        maximum_lvt_entry: u32,
        spurious_interrupt_vector: u32,
    ) -> Self {
        Self {
            flags,
            physical_address,
            apic_id,
            version,
            maximum_lvt_entry,
            spurious_interrupt_vector,
        }
    }

    /// Return whether the Local APIC EOI provider has a configured MMIO base.
    pub const fn is_configured(self) -> bool {
        self.flags & LOCAL_APIC_EOI_PROVIDER_CONFIGURED_FLAG != 0
    }

    /// Return whether hardware interrupt routing currently uses APIC EOI.
    pub const fn is_routing_active(self) -> bool {
        self.flags & LOCAL_APIC_EOI_PROVIDER_ROUTING_ACTIVE_FLAG != 0
    }

    /// Return whether the Local APIC software-enable bit is set.
    pub const fn is_software_enabled(self) -> bool {
        self.flags & LOCAL_APIC_EOI_PROVIDER_SOFTWARE_ENABLED_FLAG != 0
    }

    /// Return the Local APIC MMIO physical address.
    pub const fn physical_address(self) -> u64 {
        self.physical_address
    }

    /// Return the Local APIC identifier from the APIC ID register.
    pub const fn apic_id(self) -> u32 {
        self.apic_id
    }

    /// Return the raw Local APIC version register.
    pub const fn version(self) -> u32 {
        self.version
    }

    /// Return the maximum Local APIC LVT entry index reported by hardware.
    pub const fn maximum_lvt_entry(self) -> u32 {
        self.maximum_lvt_entry
    }

    /// Return the raw Local APIC spurious interrupt vector register.
    pub const fn spurious_interrupt_vector(self) -> u32 {
        self.spurious_interrupt_vector
    }

    /// Return the Local APIC spurious interrupt IDT vector number.
    pub const fn spurious_interrupt_vector_number(self) -> u8 {
        (self.spurious_interrupt_vector & LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_MASK) as u8
    }

    /// Return whether the Local APIC uses the diagnostic spurious vector.
    pub const fn has_diagnostic_spurious_interrupt_vector(self) -> bool {
        self.spurious_interrupt_vector_number() as u32
            == LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_NUMBER
    }
}

/// Result of activating IOAPIC interrupt routing.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IoApicRoutingActivationStatus {
    flags: u8,
    planned_entry_count: usize,
    activated_entry_count: usize,
    out_of_range_entry_count: usize,
    timer_low_readback: u32,
    timer_high_readback: u32,
    keyboard_low_readback: u32,
    keyboard_high_readback: u32,
    mouse_low_readback: u32,
    mouse_high_readback: u32,
}

impl IoApicRoutingActivationStatus {
    pub(in crate::arch::x86_64::interrupt_controller) const fn new(
        planned_entry_count: usize,
    ) -> Self {
        Self {
            flags: IOAPIC_ACTIVATION_READBACK_MATCHED_FLAG | IOAPIC_ACTIVATION_ALL_UNMASKED_FLAG,
            planned_entry_count,
            activated_entry_count: 0,
            out_of_range_entry_count: 0,
            timer_low_readback: 0,
            timer_high_readback: 0,
            keyboard_low_readback: 0,
            keyboard_high_readback: 0,
            mouse_low_readback: 0,
            mouse_high_readback: 0,
        }
    }

    /// Return the number of redirection entries from the current plan.
    pub const fn planned_entry_count(self) -> usize {
        self.planned_entry_count
    }

    /// Return the number of redirection entries activated.
    pub const fn activated_entry_count(self) -> usize {
        self.activated_entry_count
    }

    /// Return the number of planned entries outside the IOAPIC table range.
    pub const fn out_of_range_entry_count(self) -> usize {
        self.out_of_range_entry_count
    }

    /// Return whether activated entries matched their readback values.
    pub const fn readback_matches(self) -> bool {
        self.flags & IOAPIC_ACTIVATION_READBACK_MATCHED_FLAG != 0
    }

    /// Return whether all activated entries were unmasked after readback.
    pub const fn all_entries_unmasked(self) -> bool {
        self.flags & IOAPIC_ACTIVATION_ALL_UNMASKED_FLAG != 0
    }

    /// Return whether the Local APIC software-enable bit was set.
    pub const fn local_apic_software_enabled(self) -> bool {
        self.flags & IOAPIC_ACTIVATION_LOCAL_APIC_SOFTWARE_ENABLED_FLAG != 0
    }

    /// Return whether the legacy PIC lines were masked during activation.
    pub const fn legacy_pic_masked(self) -> bool {
        self.flags & IOAPIC_ACTIVATION_LEGACY_PIC_MASKED_FLAG != 0
    }

    /// Return whether IOAPIC routing is active.
    pub const fn is_routing_active(self) -> bool {
        self.flags & IOAPIC_ACTIVATION_ROUTING_ACTIVE_FLAG != 0
    }

    /// Return the timer low redirection dword readback.
    pub const fn timer_low_readback(self) -> u32 {
        self.timer_low_readback
    }

    /// Return the timer high redirection dword readback.
    pub const fn timer_high_readback(self) -> u32 {
        self.timer_high_readback
    }

    /// Return the keyboard low redirection dword readback.
    pub const fn keyboard_low_readback(self) -> u32 {
        self.keyboard_low_readback
    }

    /// Return the keyboard high redirection dword readback.
    pub const fn keyboard_high_readback(self) -> u32 {
        self.keyboard_high_readback
    }

    /// Return the mouse low redirection dword readback.
    pub const fn mouse_low_readback(self) -> u32 {
        self.mouse_low_readback
    }

    /// Return the mouse high redirection dword readback.
    pub const fn mouse_high_readback(self) -> u32 {
        self.mouse_high_readback
    }

    pub(in crate::arch::x86_64::interrupt_controller) fn mark_local_apic_software_enabled(
        &mut self,
    ) {
        self.flags |= IOAPIC_ACTIVATION_LOCAL_APIC_SOFTWARE_ENABLED_FLAG;
    }

    pub(in crate::arch::x86_64::interrupt_controller) fn mark_legacy_pic_masked(&mut self) {
        self.flags |= IOAPIC_ACTIVATION_LEGACY_PIC_MASKED_FLAG;
    }

    pub(in crate::arch::x86_64::interrupt_controller) fn mark_routing_active(&mut self) {
        self.flags |= IOAPIC_ACTIVATION_ROUTING_ACTIVE_FLAG;
    }

    pub(in crate::arch::x86_64::interrupt_controller) fn record_activated_entry(
        &mut self,
        entry: IoApicRedirectionEntry,
        low_value: u32,
        high_value: u32,
        low_readback: u32,
        high_readback: u32,
    ) {
        self.activated_entry_count += 1;
        if (low_readback & IOAPIC_REDIRECTION_LOW_READBACK_MASK) != low_value
            || high_readback != high_value
        {
            self.flags &= !IOAPIC_ACTIVATION_READBACK_MATCHED_FLAG;
        }
        if low_readback & IOAPIC_REDIRECTION_MASKED_BIT != 0 {
            self.flags &= !IOAPIC_ACTIVATION_ALL_UNMASKED_FLAG;
        }
        match entry.legacy_irq() {
            LEGACY_TIMER_IRQ => {
                self.timer_low_readback = low_readback;
                self.timer_high_readback = high_readback;
            }
            LEGACY_KEYBOARD_IRQ => {
                self.keyboard_low_readback = low_readback;
                self.keyboard_high_readback = high_readback;
            }
            LEGACY_MOUSE_IRQ => {
                self.mouse_low_readback = low_readback;
                self.mouse_high_readback = high_readback;
            }
            _ => {}
        }
    }

    pub(in crate::arch::x86_64::interrupt_controller) fn record_out_of_range_entry(&mut self) {
        self.out_of_range_entry_count += 1;
        self.flags &= !IOAPIC_ACTIVATION_READBACK_MATCHED_FLAG;
    }
}

/// Result of masking the IOAPIC timer route after Local APIC timer activation.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IoApicTimerRouteMaskStatus {
    flags: u8,
    global_system_interrupt: u32,
    table_index: u32,
    low_register: u32,
    high_register: u32,
    low_readback: u32,
    high_readback: u32,
}

impl IoApicTimerRouteMaskStatus {
    pub(in crate::arch::x86_64::interrupt_controller) fn new(
        entry: IoApicRedirectionEntry,
        low_readback: u32,
        high_readback: u32,
    ) -> Self {
        let expected_low_value = entry.low_value() | IOAPIC_REDIRECTION_MASKED_BIT;
        let mut flags = 0;
        if (low_readback & IOAPIC_REDIRECTION_LOW_READBACK_MASK) == expected_low_value
            && high_readback == entry.high_value()
        {
            flags |= IOAPIC_TIMER_MASK_READBACK_MATCHED_FLAG;
        }
        if low_readback & IOAPIC_REDIRECTION_MASKED_BIT != 0 {
            flags |= IOAPIC_TIMER_MASK_MASKED_FLAG;
        }
        if has_ioapic_routing() {
            flags |= IOAPIC_TIMER_MASK_ROUTING_ACTIVE_FLAG;
        }

        Self {
            flags,
            global_system_interrupt: entry.global_system_interrupt(),
            table_index: entry.table_index(),
            low_register: entry.low_register(),
            high_register: entry.high_register(),
            low_readback,
            high_readback,
        }
    }

    /// Return whether the masked timer route matched readback expectations.
    pub const fn readback_matches(self) -> bool {
        self.flags & IOAPIC_TIMER_MASK_READBACK_MATCHED_FLAG != 0
    }

    /// Return whether the IOAPIC timer route is masked.
    pub const fn is_masked(self) -> bool {
        self.flags & IOAPIC_TIMER_MASK_MASKED_FLAG != 0
    }

    /// Return whether IOAPIC routing remained active for other routes.
    pub const fn is_routing_active(self) -> bool {
        self.flags & IOAPIC_TIMER_MASK_ROUTING_ACTIVE_FLAG != 0
    }

    /// Return the timer global system interrupt.
    pub const fn global_system_interrupt(self) -> u32 {
        self.global_system_interrupt
    }

    /// Return the IOAPIC redirection table index for the timer route.
    pub const fn table_index(self) -> u32 {
        self.table_index
    }

    /// Return the IOAPIC low redirection register index for the timer route.
    pub const fn low_register(self) -> u32 {
        self.low_register
    }

    /// Return the IOAPIC high redirection register index for the timer route.
    pub const fn high_register(self) -> u32 {
        self.high_register
    }

    /// Return the timer low redirection dword readback.
    pub const fn low_readback(self) -> u32 {
        self.low_readback
    }

    /// Return the timer high redirection dword readback.
    pub const fn high_readback(self) -> u32 {
        self.high_readback
    }
}

/// Interrupt-controller EOI diagnostics.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct EndOfInterruptStatus {
    ioapic_routing_active: bool,
    apic_count: u64,
    legacy_count: u64,
}

impl EndOfInterruptStatus {
    pub(in crate::arch::x86_64::interrupt_controller) const fn new(
        ioapic_routing_active: bool,
        apic_count: u64,
        legacy_count: u64,
    ) -> Self {
        Self {
            ioapic_routing_active,
            apic_count,
            legacy_count,
        }
    }

    /// Return whether IOAPIC routing is active.
    pub const fn is_ioapic_routing_active(self) -> bool {
        self.ioapic_routing_active
    }

    /// Return the number of Local APIC EOI writes.
    pub const fn apic_count(self) -> u64 {
        self.apic_count
    }

    /// Return the number of legacy PIC EOI notifications.
    pub const fn legacy_count(self) -> u64 {
        self.legacy_count
    }
}

/// Legacy PIC boundary diagnostics for the selected interrupt backend.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LegacyPicBoundaryStatus {
    flags: u8,
    master_mask: u8,
    slave_mask: u8,
}

impl LegacyPicBoundaryStatus {
    pub(in crate::arch::x86_64::interrupt_controller) const fn new(
        flags: u8,
        master_mask: u8,
        slave_mask: u8,
    ) -> Self {
        Self {
            flags,
            master_mask,
            slave_mask,
        }
    }

    /// Return whether the legacy PIC backend was initialized.
    pub const fn is_initialized(self) -> bool {
        self.flags & LEGACY_PIC_INITIALIZED_FLAG != 0
    }

    /// Return whether legacy PIC fallback delivery is enabled.
    pub const fn is_fallback_enabled(self) -> bool {
        self.flags & LEGACY_PIC_FALLBACK_ENABLED_FLAG != 0
    }

    /// Return whether the legacy PIC is masked for APIC routing.
    pub const fn is_masked_for_apic_routing(self) -> bool {
        self.flags & LEGACY_PIC_MASKED_FOR_APIC_ROUTING_FLAG != 0
    }

    /// Return the current master PIC interrupt mask.
    pub const fn master_mask(self) -> u8 {
        self.master_mask
    }

    /// Return the current slave PIC interrupt mask.
    pub const fn slave_mask(self) -> u8 {
        self.slave_mask
    }
}
