//! Interrupt controller configuration and diagnostics records.

use super::{
    has_ioapic_routing, has_local_apic, redirection_entry_for_legacy_irq,
    APIC_PROVIDER_CONFIGURED_FLAG, APIC_PROVIDER_LOCAL_APIC_SUPPORTED_FLAG,
    APIC_PROVIDER_ROUTING_ACTIVE_FLAG, APIC_PROVIDER_TRUNCATED_FLAG,
    IOAPIC_ACTIVATION_ALL_UNMASKED_FLAG, IOAPIC_ACTIVATION_LEGACY_PIC_MASKED_FLAG,
    IOAPIC_ACTIVATION_LOCAL_APIC_SOFTWARE_ENABLED_FLAG, IOAPIC_ACTIVATION_READBACK_MATCHED_FLAG,
    IOAPIC_ACTIVATION_ROUTING_ACTIVE_FLAG, IOAPIC_REDIRECTION_ACTIVE_LOW_BIT,
    IOAPIC_REDIRECTION_LEVEL_TRIGGERED_BIT, IOAPIC_REDIRECTION_LOW_READBACK_MASK,
    IOAPIC_REDIRECTION_MASKED_BIT, IOAPIC_REDIRECTION_PLAN_CAPACITY,
    IOAPIC_STAGING_ALL_MASKED_FLAG, IOAPIC_STAGING_READBACK_MATCHED_FLAG,
    IOAPIC_TIMER_MASK_MASKED_FLAG, IOAPIC_TIMER_MASK_READBACK_MATCHED_FLAG,
    IOAPIC_TIMER_MASK_ROUTING_ACTIVE_FLAG, KEYBOARD_INTERRUPT_VECTOR, LEGACY_IRQ_ROUTE_CAPACITY,
    LEGACY_KEYBOARD_IRQ, LEGACY_MOUSE_IRQ, LEGACY_PIC_FALLBACK_ENABLED_FLAG,
    LEGACY_PIC_INITIALIZED_FLAG, LEGACY_PIC_MASKED_FOR_APIC_ROUTING_FLAG, LEGACY_TIMER_IRQ,
    LOCAL_APIC_EOI_PROVIDER_CONFIGURED_FLAG, LOCAL_APIC_EOI_PROVIDER_ROUTING_ACTIVE_FLAG,
    LOCAL_APIC_EOI_PROVIDER_SOFTWARE_ENABLED_FLAG, LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_MASK,
    LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_NUMBER, MOUSE_INTERRUPT_VECTOR, TIMER_INTERRUPT_VECTOR,
};
/// Available interrupt controller backends.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InterruptControllerKind {
    /// Legacy 8259 chained interrupt controllers.
    Legacy8259,
    /// Local APIC plus IOAPIC-capable hardware.
    LocalApicIoApic,
}

/// Physical MMIO base address for Local APIC and IOAPIC register windows.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ApicMmioAddress(u64);

impl ApicMmioAddress {
    /// Create an APIC MMIO physical address from an ACPI-reported base.
    pub const fn new(physical_address: u64) -> Self {
        Self(physical_address)
    }

    /// Return whether the APIC MMIO address is unavailable.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Return the raw physical address for diagnostics or final MMIO mapping.
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Return the APIC MMIO address as a host pointer-sized integer.
    ///
    /// # Panics
    ///
    /// Panics if the APIC MMIO address does not fit in `usize`.
    pub(in crate::arch::x86_64) fn as_usize(self) -> usize {
        usize::try_from(self.0).expect("APIC MMIO address must fit in usize")
    }
}

/// Local APIC configuration supplied by the kernel composition root.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LocalApicConfiguration {
    physical_address: ApicMmioAddress,
    apic_id: u32,
    enabled: bool,
    online_capable: bool,
}

impl LocalApicConfiguration {
    const EMPTY: Self = Self::new(ApicMmioAddress::new(0), 0, false, false);

    /// Create a Local APIC configuration record.
    pub const fn new(
        physical_address: ApicMmioAddress,
        apic_id: u32,
        enabled: bool,
        online_capable: bool,
    ) -> Self {
        Self {
            physical_address,
            apic_id,
            enabled,
            online_capable,
        }
    }

    /// Return the Local APIC MMIO physical address.
    pub const fn physical_address(self) -> ApicMmioAddress {
        self.physical_address
    }

    /// Return the Local APIC identifier.
    pub const fn apic_id(self) -> u32 {
        self.apic_id
    }

    /// Return whether this Local APIC is enabled.
    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    /// Return whether this Local APIC can be brought online later.
    pub const fn is_online_capable(self) -> bool {
        self.online_capable
    }
}

/// IOAPIC configuration supplied by the kernel composition root.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IoApicConfiguration {
    id: u8,
    physical_address: ApicMmioAddress,
    global_system_interrupt_base: u32,
}

impl IoApicConfiguration {
    const EMPTY: Self = Self::new(0, ApicMmioAddress::new(0), 0);

    /// Create an IOAPIC configuration record.
    pub const fn new(
        id: u8,
        physical_address: ApicMmioAddress,
        global_system_interrupt_base: u32,
    ) -> Self {
        Self {
            id,
            physical_address,
            global_system_interrupt_base,
        }
    }

    /// Return the IOAPIC identifier.
    pub const fn id(self) -> u8 {
        self.id
    }

    /// Return the IOAPIC MMIO physical address.
    pub const fn physical_address(self) -> ApicMmioAddress {
        self.physical_address
    }

    /// Return the first global system interrupt handled by this IOAPIC.
    pub const fn global_system_interrupt_base(self) -> u32 {
        self.global_system_interrupt_base
    }
}

/// Legacy IRQ to global-system-interrupt route.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LegacyIrqRoute {
    legacy_irq: u8,
    global_system_interrupt: u32,
    flags: u16,
}

impl LegacyIrqRoute {
    const EMPTY: Self = Self::new(0, 0, 0);

    /// Create a legacy IRQ routing record.
    pub const fn new(legacy_irq: u8, global_system_interrupt: u32, flags: u16) -> Self {
        Self {
            legacy_irq,
            global_system_interrupt,
            flags,
        }
    }

    /// Return the legacy IRQ source line.
    pub const fn legacy_irq(self) -> u8 {
        self.legacy_irq
    }

    /// Return the global system interrupt used for this IRQ source line.
    pub const fn global_system_interrupt(self) -> u32 {
        self.global_system_interrupt
    }

    /// Return the raw ACPI polarity and trigger-mode flags.
    pub const fn flags(self) -> u16 {
        self.flags
    }
}

/// Planned IOAPIC redirection-table entry.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IoApicRedirectionEntry {
    legacy_irq: u8,
    global_system_interrupt: u32,
    vector: u8,
    table_index: u32,
    low_register: u32,
    low_value: u32,
    high_value: u32,
}

impl IoApicRedirectionEntry {
    const EMPTY: Self = Self::new(0, 0, 0, 0, 0, 0, 0);

    pub(in crate::arch::x86_64::interrupt_controller) const fn new(
        legacy_irq: u8,
        global_system_interrupt: u32,
        vector: u8,
        table_index: u32,
        low_register: u32,
        low_value: u32,
        high_value: u32,
    ) -> Self {
        Self {
            legacy_irq,
            global_system_interrupt,
            vector,
            table_index,
            low_register,
            low_value,
            high_value,
        }
    }

    /// Return the legacy IRQ source line for this route.
    pub const fn legacy_irq(self) -> u8 {
        self.legacy_irq
    }

    /// Return the global system interrupt for this route.
    pub const fn global_system_interrupt(self) -> u32 {
        self.global_system_interrupt
    }

    /// Return the IDT vector programmed into the redirection entry.
    pub const fn vector(self) -> u8 {
        self.vector
    }

    /// Return the IOAPIC redirection table index.
    pub const fn table_index(self) -> u32 {
        self.table_index
    }

    /// Return the IOAPIC register index for the low redirection dword.
    pub const fn low_register(self) -> u32 {
        self.low_register
    }

    /// Return the IOAPIC register index for the high redirection dword.
    pub const fn high_register(self) -> u32 {
        self.low_register + 1
    }

    /// Return the raw low redirection dword to program.
    pub const fn low_value(self) -> u32 {
        self.low_value
    }

    /// Return the raw high redirection dword to program.
    pub const fn high_value(self) -> u32 {
        self.high_value
    }

    /// Return whether the route uses active-low interrupt polarity.
    pub const fn is_active_low(self) -> bool {
        self.low_value & IOAPIC_REDIRECTION_ACTIVE_LOW_BIT != 0
    }

    /// Return whether the route uses level-triggered delivery.
    pub const fn is_level_triggered(self) -> bool {
        self.low_value & IOAPIC_REDIRECTION_LEVEL_TRIGGERED_BIT != 0
    }

    /// Return whether the route is masked in the planned low dword.
    pub const fn is_masked(self) -> bool {
        self.low_value & IOAPIC_REDIRECTION_MASKED_BIT != 0
    }
}

/// Planned IOAPIC redirection entries for the current legacy IRQ sources.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IoApicRedirectionPlan {
    entries: [IoApicRedirectionEntry; IOAPIC_REDIRECTION_PLAN_CAPACITY],
    entry_count: usize,
    truncated: bool,
}

impl IoApicRedirectionPlan {
    const fn new() -> Self {
        Self {
            entries: [IoApicRedirectionEntry::EMPTY; IOAPIC_REDIRECTION_PLAN_CAPACITY],
            entry_count: 0,
            truncated: false,
        }
    }

    pub(super) fn from_configuration(configuration: &ApicRoutingConfiguration) -> Self {
        let mut plan = Self::new();
        let destination_apic_id = configuration.local_apic().apic_id();
        plan.push_legacy_irq_entry(
            configuration,
            LEGACY_TIMER_IRQ,
            TIMER_INTERRUPT_VECTOR,
            destination_apic_id,
        );
        plan.push_legacy_irq_entry(
            configuration,
            LEGACY_KEYBOARD_IRQ,
            KEYBOARD_INTERRUPT_VECTOR,
            destination_apic_id,
        );
        plan.push_legacy_irq_entry(
            configuration,
            LEGACY_MOUSE_IRQ,
            MOUSE_INTERRUPT_VECTOR,
            destination_apic_id,
        );
        plan
    }

    /// Return the number of retained redirection entries.
    pub const fn entry_count(self) -> usize {
        self.entry_count
    }

    /// Return whether redirection entries exceeded the retained capacity.
    pub const fn is_truncated(self) -> bool {
        self.truncated
    }

    /// Return a redirection entry by retained index.
    pub const fn entry(self, index: usize) -> Option<IoApicRedirectionEntry> {
        if index < self.entry_count {
            Some(self.entries[index])
        } else {
            None
        }
    }

    /// Return a redirection entry for a legacy IRQ source line.
    pub fn entry_for_legacy_irq(self, legacy_irq: u8) -> Option<IoApicRedirectionEntry> {
        let mut index = 0;
        while index < self.entry_count {
            let entry = self.entries[index];
            if entry.legacy_irq() == legacy_irq {
                return Some(entry);
            }
            index += 1;
        }
        None
    }

    fn push_legacy_irq_entry(
        &mut self,
        configuration: &ApicRoutingConfiguration,
        legacy_irq: u8,
        vector: u8,
        destination_apic_id: u32,
    ) {
        let Some(entry) = redirection_entry_for_legacy_irq(
            configuration,
            legacy_irq,
            vector,
            destination_apic_id,
        ) else {
            self.truncated = true;
            return;
        };

        if self.entry_count < IOAPIC_REDIRECTION_PLAN_CAPACITY {
            self.entries[self.entry_count] = entry;
            self.entry_count += 1;
        } else {
            self.truncated = true;
        }
    }
}

/// Architecture-owned APIC routing configuration derived by `main.rs`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ApicRoutingConfiguration {
    local_apic: LocalApicConfiguration,
    ioapic: IoApicConfiguration,
    legacy_irq_routes: [LegacyIrqRoute; LEGACY_IRQ_ROUTE_CAPACITY],
    legacy_irq_route_count: usize,
    truncated: bool,
}

impl ApicRoutingConfiguration {
    pub(super) const EMPTY: Self =
        Self::new(LocalApicConfiguration::EMPTY, IoApicConfiguration::EMPTY);

    /// Create an APIC routing configuration with no legacy IRQ overrides.
    pub const fn new(local_apic: LocalApicConfiguration, ioapic: IoApicConfiguration) -> Self {
        Self {
            local_apic,
            ioapic,
            legacy_irq_routes: [LegacyIrqRoute::EMPTY; LEGACY_IRQ_ROUTE_CAPACITY],
            legacy_irq_route_count: 0,
            truncated: false,
        }
    }

    /// Return the configured Local APIC record.
    pub const fn local_apic(self) -> LocalApicConfiguration {
        self.local_apic
    }

    /// Return the configured IOAPIC record.
    pub const fn ioapic(self) -> IoApicConfiguration {
        self.ioapic
    }

    /// Return the number of retained legacy IRQ override routes.
    pub const fn legacy_irq_route_count(self) -> usize {
        self.legacy_irq_route_count
    }

    /// Return whether legacy IRQ route records exceeded the retained capacity.
    pub const fn is_truncated(self) -> bool {
        self.truncated
    }

    /// Add one legacy IRQ override route.
    pub fn push_legacy_irq_route(&mut self, route: LegacyIrqRoute) {
        if self.legacy_irq_route_count < LEGACY_IRQ_ROUTE_CAPACITY {
            self.legacy_irq_routes[self.legacy_irq_route_count] = route;
            self.legacy_irq_route_count += 1;
        } else {
            self.truncated = true;
        }
    }

    /// Return a legacy IRQ override route for one source line.
    pub fn legacy_irq_route_for_irq(self, legacy_irq: u8) -> Option<LegacyIrqRoute> {
        let mut index = 0;
        while index < self.legacy_irq_route_count {
            let route = self.legacy_irq_routes[index];
            if route.legacy_irq() == legacy_irq {
                return Some(route);
            }
            index += 1;
        }
        None
    }

    /// Return the global system interrupt for a legacy IRQ source line.
    pub fn global_system_interrupt_for_legacy_irq(self, legacy_irq: u8) -> u32 {
        self.legacy_irq_route_for_irq(legacy_irq).map_or(
            u32::from(legacy_irq),
            LegacyIrqRoute::global_system_interrupt,
        )
    }
}

mod status;

pub use status::{
    ApicRoutingProviderStatus, EndOfInterruptStatus, IoApicRedirectionStagingStatus,
    IoApicRoutingActivationStatus, IoApicTimerRouteMaskStatus, LegacyPicBoundaryStatus,
    LocalApicEoiProviderStatus,
};
