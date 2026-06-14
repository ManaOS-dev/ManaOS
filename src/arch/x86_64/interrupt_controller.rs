//! Interrupt controller selection and initialization.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use pic8259::ChainedPics;
use spin::Mutex;

const INTERRUPT_CONTROLLER_1_OFFSET: u8 = 32;
const INTERRUPT_CONTROLLER_2_OFFSET: u8 = INTERRUPT_CONTROLLER_1_OFFSET + 8;
const LEGACY_IRQ_ROUTE_CAPACITY: usize = 16;
const IOAPIC_REDIRECTION_PLAN_CAPACITY: usize = 3;
const APIC_PROVIDER_CONFIGURED_FLAG: u8 = 1;
const APIC_PROVIDER_ROUTING_ACTIVE_FLAG: u8 = 1 << 1;
const APIC_PROVIDER_LOCAL_APIC_SUPPORTED_FLAG: u8 = 1 << 2;
const APIC_PROVIDER_TRUNCATED_FLAG: u8 = 1 << 3;
const LOCAL_APIC_EOI_PROVIDER_CONFIGURED_FLAG: u8 = 1;
const LOCAL_APIC_EOI_PROVIDER_ROUTING_ACTIVE_FLAG: u8 = 1 << 1;
const LOCAL_APIC_EOI_PROVIDER_SOFTWARE_ENABLED_FLAG: u8 = 1 << 2;
const LEGACY_PIC_INITIALIZED_FLAG: u8 = 1;
const LEGACY_PIC_FALLBACK_ENABLED_FLAG: u8 = 1 << 1;
const LEGACY_PIC_MASKED_FOR_APIC_ROUTING_FLAG: u8 = 1 << 2;
const LEGACY_PIC_MASTER_APIC_MASK: u8 = 0xff;
const LEGACY_PIC_SLAVE_APIC_MASK: u8 = 0xff;
const LEGACY_PIC_MASTER_FALLBACK_MASK: u8 = 0xf8;
const LEGACY_PIC_SLAVE_FALLBACK_MASK: u8 = 0xef;
const IOAPIC_ACTIVATION_READBACK_MATCHED_FLAG: u8 = 1;
const IOAPIC_ACTIVATION_ALL_UNMASKED_FLAG: u8 = 1 << 1;
const IOAPIC_ACTIVATION_LOCAL_APIC_SOFTWARE_ENABLED_FLAG: u8 = 1 << 2;
const IOAPIC_ACTIVATION_LEGACY_PIC_MASKED_FLAG: u8 = 1 << 3;
const IOAPIC_ACTIVATION_ROUTING_ACTIVE_FLAG: u8 = 1 << 4;
const IOAPIC_TIMER_MASK_READBACK_MATCHED_FLAG: u8 = 1;
const IOAPIC_TIMER_MASK_MASKED_FLAG: u8 = 1 << 1;
const IOAPIC_TIMER_MASK_ROUTING_ACTIVE_FLAG: u8 = 1 << 2;
const LOCAL_APIC_ID_REGISTER: usize = 0x20;
const LOCAL_APIC_VERSION_REGISTER: usize = 0x30;
const LOCAL_APIC_EOI_REGISTER: usize = 0xb0;
const LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_REGISTER: usize = 0xf0;
const LOCAL_APIC_EOI_VALUE: u32 = 0;
const LOCAL_APIC_ID_SHIFT: u32 = 24;
const LOCAL_APIC_ID_MASK: u32 = 0xff;
const LOCAL_APIC_VERSION_MAX_LVT_ENTRY_SHIFT: u32 = 16;
const LOCAL_APIC_VERSION_MAX_LVT_ENTRY_MASK: u32 = 0xff;
const LOCAL_APIC_SOFTWARE_ENABLE_BIT: u32 = 1 << 8;
const IOAPIC_STAGING_READBACK_MATCHED_FLAG: u8 = 1;
const IOAPIC_STAGING_ALL_MASKED_FLAG: u8 = 1 << 1;
const IOAPIC_REGISTER_SELECT_OFFSET: usize = 0x00;
const IOAPIC_REGISTER_WINDOW_OFFSET: usize = 0x10;
const IOAPIC_VERSION_REGISTER: u32 = 0x01;
const IOAPIC_VERSION_MAX_REDIRECTION_ENTRY_SHIFT: u32 = 16;
const IOAPIC_VERSION_MAX_REDIRECTION_ENTRY_MASK: u32 = 0xff;
const LEGACY_TIMER_IRQ: u8 = 0;
const LEGACY_KEYBOARD_IRQ: u8 = 1;
const LEGACY_MOUSE_IRQ: u8 = 12;
const TIMER_INTERRUPT_VECTOR: u8 = INTERRUPT_CONTROLLER_1_OFFSET;
const KEYBOARD_INTERRUPT_VECTOR: u8 = INTERRUPT_CONTROLLER_1_OFFSET + 1;
const MOUSE_INTERRUPT_VECTOR: u8 = INTERRUPT_CONTROLLER_1_OFFSET + LEGACY_MOUSE_IRQ;
const IOAPIC_REDIRECTION_TABLE_BASE_REGISTER: u32 = 0x10;
const IOAPIC_REDIRECTION_VECTOR_MASK: u32 = 0xff;
const IOAPIC_REDIRECTION_DELIVERY_MODE_MASK: u32 = 0b111 << 8;
const IOAPIC_REDIRECTION_DESTINATION_MODE_BIT: u32 = 1 << 11;
const IOAPIC_REDIRECTION_ACTIVE_LOW_BIT: u32 = 1 << 13;
const IOAPIC_REDIRECTION_LEVEL_TRIGGERED_BIT: u32 = 1 << 15;
const IOAPIC_REDIRECTION_MASKED_BIT: u32 = 1 << 16;
const IOAPIC_REDIRECTION_LOW_READBACK_MASK: u32 = IOAPIC_REDIRECTION_VECTOR_MASK
    | IOAPIC_REDIRECTION_DELIVERY_MODE_MASK
    | IOAPIC_REDIRECTION_DESTINATION_MODE_BIT
    | IOAPIC_REDIRECTION_ACTIVE_LOW_BIT
    | IOAPIC_REDIRECTION_LEVEL_TRIGGERED_BIT
    | IOAPIC_REDIRECTION_MASKED_BIT;
const IOAPIC_DESTINATION_SHIFT: u32 = 24;
const ACPI_INTERRUPT_POLARITY_MASK: u16 = 0b11;
const ACPI_INTERRUPT_ACTIVE_LOW: u16 = 0b11;
const ACPI_INTERRUPT_TRIGGER_MASK: u16 = 0b11 << 2;
const ACPI_INTERRUPT_LEVEL_TRIGGERED: u16 = 0b11 << 2;

static LEGACY_INTERRUPT_CONTROLLERS: Mutex<ChainedPics> =
    // SAFETY: The offsets reserve CPU exception vectors and match the configured
    // interrupt descriptor table entries.
    Mutex::new(unsafe {
        ChainedPics::new(INTERRUPT_CONTROLLER_1_OFFSET, INTERRUPT_CONTROLLER_2_OFFSET)
    });

static APIC_ROUTING_PROVIDER: Mutex<ApicRoutingProviderState> =
    Mutex::new(ApicRoutingProviderState::new());
static LOCAL_APIC_EOI_BASE_ADDRESS: AtomicUsize = AtomicUsize::new(0);
static IOAPIC_ROUTING_ACTIVE: AtomicBool = AtomicBool::new(false);
static APIC_END_OF_INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);
static LEGACY_END_OF_INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);
static LEGACY_PIC_STATE_FLAGS: AtomicU8 = AtomicU8::new(0);
static LEGACY_PIC_MASTER_MASK: AtomicU8 = AtomicU8::new(LEGACY_PIC_MASTER_APIC_MASK);
static LEGACY_PIC_SLAVE_MASK: AtomicU8 = AtomicU8::new(LEGACY_PIC_SLAVE_APIC_MASK);

/// Available interrupt controller backends.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InterruptControllerKind {
    /// Legacy 8259 chained interrupt controllers.
    Legacy8259,
    /// Local APIC plus IOAPIC-capable hardware.
    LocalApicIoApic,
}

/// Local APIC configuration supplied by the kernel composition root.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LocalApicConfiguration {
    physical_address: u64,
    apic_id: u32,
    enabled: bool,
    online_capable: bool,
}

impl LocalApicConfiguration {
    const EMPTY: Self = Self::new(0, 0, false, false);

    /// Create a Local APIC configuration record.
    pub const fn new(
        physical_address: u64,
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
    pub const fn physical_address(self) -> u64 {
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
    physical_address: u64,
    global_system_interrupt_base: u32,
}

impl IoApicConfiguration {
    const EMPTY: Self = Self::new(0, 0, 0);

    /// Create an IOAPIC configuration record.
    pub const fn new(id: u8, physical_address: u64, global_system_interrupt_base: u32) -> Self {
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
    pub const fn physical_address(self) -> u64 {
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

    const fn new(
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

    fn from_configuration(configuration: &ApicRoutingConfiguration) -> Self {
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
    const EMPTY: Self = Self::new(LocalApicConfiguration::EMPTY, IoApicConfiguration::EMPTY);

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
    const fn unavailable() -> Self {
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

    fn from_configuration(configuration: &ApicRoutingConfiguration) -> Self {
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
    const fn new(version: u32, maximum_redirection_entry: u32, planned_entry_count: usize) -> Self {
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

    fn record_staged_entry(
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

    fn record_out_of_range_entry(&mut self) {
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
    const fn new(
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
    const fn new(planned_entry_count: usize) -> Self {
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

    fn mark_local_apic_software_enabled(&mut self) {
        self.flags |= IOAPIC_ACTIVATION_LOCAL_APIC_SOFTWARE_ENABLED_FLAG;
    }

    fn mark_legacy_pic_masked(&mut self) {
        self.flags |= IOAPIC_ACTIVATION_LEGACY_PIC_MASKED_FLAG;
    }

    fn mark_routing_active(&mut self) {
        self.flags |= IOAPIC_ACTIVATION_ROUTING_ACTIVE_FLAG;
    }

    fn record_activated_entry(
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

    fn record_out_of_range_entry(&mut self) {
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
    fn new(entry: IoApicRedirectionEntry, low_readback: u32, high_readback: u32) -> Self {
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
    const fn new(ioapic_routing_active: bool, apic_count: u64, legacy_count: u64) -> Self {
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
    const fn new(flags: u8, master_mask: u8, slave_mask: u8) -> Self {
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct ApicRoutingProviderState {
    configured: bool,
    configuration: ApicRoutingConfiguration,
}

impl ApicRoutingProviderState {
    const fn new() -> Self {
        Self {
            configured: false,
            configuration: ApicRoutingConfiguration::EMPTY,
        }
    }
}

/// Return the interrupt controller backend preferred by CPU capability.
pub fn get_preferred_kind() -> InterruptControllerKind {
    if super::has_apic() {
        InterruptControllerKind::LocalApicIoApic
    } else {
        InterruptControllerKind::Legacy8259
    }
}

/// Initialize the interrupt-controller backend required for early boot.
///
/// APIC-capable boots keep the legacy PIC masked until IOAPIC routing is
/// activated. The legacy backend is initialized only when APIC routing provider
/// data is unavailable.
///
/// # Safety
///
/// Must be called while interrupts are disabled.
pub unsafe fn initialize_interrupt_controller_backend() -> LegacyPicBoundaryStatus {
    if should_mask_legacy_pic_for_apic_backend() {
        mask_legacy_interrupts_for_apic_routing();
    } else {
        // SAFETY: The caller guarantees that CPU interrupts are disabled during
        // early architecture initialization.
        unsafe {
            initialize_legacy();
        }
    }
    get_legacy_pic_boundary_status()
}

/// Configure the architecture-owned APIC routing provider data.
pub fn configure_apic_routing_provider(configuration: &ApicRoutingConfiguration) {
    let mut provider = APIC_ROUTING_PROVIDER.lock();
    provider.configured = true;
    provider.configuration = *configuration;
    let local_apic_address = usize::try_from(configuration.local_apic().physical_address())
        .expect("Local APIC MMIO address must fit in usize");
    LOCAL_APIC_EOI_BASE_ADDRESS.store(local_apic_address, Ordering::Release);
}

/// Return whether APIC routing provider data has been configured.
pub fn is_apic_routing_provider_configured() -> bool {
    APIC_ROUTING_PROVIDER.lock().configured
}

/// Return the APIC routing provider status for diagnostics.
pub fn get_apic_routing_provider_status() -> ApicRoutingProviderStatus {
    let provider = APIC_ROUTING_PROVIDER.lock();
    if provider.configured {
        ApicRoutingProviderStatus::from_configuration(&provider.configuration)
    } else {
        ApicRoutingProviderStatus::unavailable()
    }
}

/// Stage the planned IOAPIC redirection entries as masked routes.
///
/// The function writes only masked redirection entries and leaves active
/// interrupt routing disabled. It is intended to prove MMIO access and table
/// programming before APIC EOI handling replaces the legacy PIC path.
///
/// # Safety
///
/// The configured IOAPIC MMIO physical page must be identity-mapped as
/// writable uncached kernel memory, and the caller must ensure no other code is
/// concurrently programming the same IOAPIC registers.
pub unsafe fn stage_masked_ioapic_redirection_entries() -> Option<IoApicRedirectionStagingStatus> {
    let configuration = {
        let provider = APIC_ROUTING_PROVIDER.lock();
        if !provider.configured {
            return None;
        }
        provider.configuration
    };

    let plan = IoApicRedirectionPlan::from_configuration(&configuration);
    let ioapic = configuration.ioapic();
    let registers = IoApicRegisters::new(ioapic.physical_address());
    // SAFETY: The IOAPIC register window is mapped and the version register is
    // a read-only architectural IOAPIC register.
    let version = unsafe { registers.read(IOAPIC_VERSION_REGISTER) };
    let maximum_redirection_entry = maximum_redirection_entry_from_version(version);
    let mut status =
        IoApicRedirectionStagingStatus::new(version, maximum_redirection_entry, plan.entry_count());

    let mut index = 0;
    while index < plan.entry_count() {
        let entry = plan
            .entry(index)
            .expect("retained IOAPIC redirection plan entry must exist");
        if entry.table_index() > maximum_redirection_entry {
            status.record_out_of_range_entry();
            index += 1;
            continue;
        }

        let low_value = entry.low_value() | IOAPIC_REDIRECTION_MASKED_BIT;
        let high_value = entry.high_value();
        // SAFETY: The IOAPIC register window is mapped, and the redirection
        // registers were range-checked against the IOAPIC version register.
        unsafe {
            registers.write(entry.high_register(), high_value);
            registers.write(entry.low_register(), low_value);
        }
        // SAFETY: The same range-checked redirection registers were just
        // programmed and can be read back through the mapped IOAPIC window.
        let high_readback = unsafe { registers.read(entry.high_register()) };
        // SAFETY: The same range-checked redirection registers were just
        // programmed and can be read back through the mapped IOAPIC window.
        let low_readback = unsafe { registers.read(entry.low_register()) };
        status.record_staged_entry(entry, low_value, high_value, low_readback, high_readback);
        index += 1;
    }

    Some(status)
}

/// Inspect Local APIC registers needed before APIC EOI can replace PIC EOI.
///
/// # Safety
///
/// The configured Local APIC MMIO physical page must be identity-mapped as
/// readable kernel memory.
pub unsafe fn inspect_local_apic_eoi_provider() -> Option<LocalApicEoiProviderStatus> {
    let base_address = LOCAL_APIC_EOI_BASE_ADDRESS.load(Ordering::Acquire);
    if base_address == 0 {
        return None;
    }

    let registers = LocalApicRegisters::new(base_address);
    // SAFETY: The caller guarantees the Local APIC MMIO page is mapped, and the
    // APIC ID register is a readable architectural Local APIC register.
    let id_register = unsafe { registers.read(LOCAL_APIC_ID_REGISTER) };
    // SAFETY: The caller guarantees the Local APIC MMIO page is mapped, and the
    // APIC version register is a readable architectural Local APIC register.
    let version = unsafe { registers.read(LOCAL_APIC_VERSION_REGISTER) };
    // SAFETY: The caller guarantees the Local APIC MMIO page is mapped, and the
    // spurious vector register is readable for diagnostics.
    let spurious_interrupt_vector =
        unsafe { registers.read(LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_REGISTER) };
    let mut flags = LOCAL_APIC_EOI_PROVIDER_CONFIGURED_FLAG;
    if has_ioapic_routing() {
        flags |= LOCAL_APIC_EOI_PROVIDER_ROUTING_ACTIVE_FLAG;
    }
    if spurious_interrupt_vector & LOCAL_APIC_SOFTWARE_ENABLE_BIT != 0 {
        flags |= LOCAL_APIC_EOI_PROVIDER_SOFTWARE_ENABLED_FLAG;
    }

    Some(LocalApicEoiProviderStatus::new(
        flags,
        u64::try_from(base_address).expect("Local APIC MMIO address must fit in u64"),
        local_apic_id_from_register(id_register),
        version,
        maximum_lvt_entry_from_version(version),
        spurious_interrupt_vector,
    ))
}

/// Activate IOAPIC routing for the planned legacy interrupt sources.
///
/// This unmasks the timer, keyboard, and mouse IOAPIC redirection entries,
/// masks the legacy PIC, and switches EOI dispatch to the Local APIC.
///
/// # Safety
///
/// Interrupts must be disabled. The Local APIC and IOAPIC MMIO pages must be
/// identity-mapped as writable uncached kernel memory, and no other code may
/// concurrently program those interrupt-controller registers.
pub unsafe fn activate_ioapic_routing() -> Option<IoApicRoutingActivationStatus> {
    let configuration = {
        let provider = APIC_ROUTING_PROVIDER.lock();
        if !provider.configured {
            return None;
        }
        provider.configuration
    };

    let plan = IoApicRedirectionPlan::from_configuration(&configuration);
    let local_apic_base_address = LOCAL_APIC_EOI_BASE_ADDRESS.load(Ordering::Acquire);
    if local_apic_base_address == 0 {
        return None;
    }

    let local_apic_registers = LocalApicRegisters::new(local_apic_base_address);
    let ioapic_registers = IoApicRegisters::new(configuration.ioapic().physical_address());
    let mut status = IoApicRoutingActivationStatus::new(plan.entry_count());

    // SAFETY: The caller guarantees Local APIC MMIO is mapped and exclusively
    // programmed during interrupt-controller activation.
    if unsafe { ensure_local_apic_software_enabled(&local_apic_registers) } {
        status.mark_local_apic_software_enabled();
    }
    // SAFETY: The caller guarantees IOAPIC MMIO is mapped and exclusively
    // programmed during interrupt-controller activation.
    unsafe {
        activate_ioapic_redirection_plan(&ioapic_registers, plan, &mut status);
    }

    assert!(
        status.readback_matches()
            && status.all_entries_unmasked()
            && status.activated_entry_count() == status.planned_entry_count()
            && status.out_of_range_entry_count() == 0
            && status.local_apic_software_enabled(),
        "IOAPIC routing activation preconditions failed"
    );

    mask_legacy_interrupts_for_apic_routing();
    status.mark_legacy_pic_masked();
    IOAPIC_ROUTING_ACTIVE.store(true, Ordering::Release);
    status.mark_routing_active();
    Some(status)
}

/// Mask the IOAPIC timer route after Local APIC timer ticks are available.
///
/// Keyboard and mouse IOAPIC routes remain active. This removes the PIT timer
/// from the scheduler tick path while preserving APIC EOI dispatch for all
/// active interrupt sources.
///
/// # Safety
///
/// Interrupts must be disabled. IOAPIC routing must already be active, the
/// IOAPIC MMIO page must remain mapped as writable kernel memory, and no other
/// code may concurrently program IOAPIC redirection entries.
pub unsafe fn mask_ioapic_timer_route_for_local_apic_timer() -> Option<IoApicTimerRouteMaskStatus> {
    if !has_ioapic_routing() {
        return None;
    }
    let configuration = {
        let provider = APIC_ROUTING_PROVIDER.lock();
        if !provider.configured {
            return None;
        }
        provider.configuration
    };

    let plan = IoApicRedirectionPlan::from_configuration(&configuration);
    let entry = plan.entry_for_legacy_irq(LEGACY_TIMER_IRQ)?;
    let registers = IoApicRegisters::new(configuration.ioapic().physical_address());
    // SAFETY: The caller guarantees IOAPIC MMIO is mapped and exclusively
    // programmed while interrupts are disabled.
    let version = unsafe { registers.read(IOAPIC_VERSION_REGISTER) };
    let maximum_redirection_entry = maximum_redirection_entry_from_version(version);
    assert!(
        entry.table_index() <= maximum_redirection_entry,
        "IOAPIC timer redirection entry must be in range before masking"
    );

    let low_value = entry.low_value() | IOAPIC_REDIRECTION_MASKED_BIT;
    let high_value = entry.high_value();
    // SAFETY: The timer redirection entry was range-checked against the IOAPIC
    // version register, and the caller guarantees exclusive MMIO access.
    unsafe {
        registers.write(entry.high_register(), high_value);
        registers.write(entry.low_register(), low_value);
    }
    // SAFETY: The same range-checked timer redirection registers were just
    // programmed and can be read back through the mapped IOAPIC window.
    let high_readback = unsafe { registers.read(entry.high_register()) };
    // SAFETY: The same range-checked timer redirection registers were just
    // programmed and can be read back through the mapped IOAPIC window.
    let low_readback = unsafe { registers.read(entry.low_register()) };
    Some(IoApicTimerRouteMaskStatus::new(
        entry,
        low_readback,
        high_readback,
    ))
}

/// Return interrupt-controller EOI counters.
pub fn get_end_of_interrupt_status() -> EndOfInterruptStatus {
    EndOfInterruptStatus::new(
        has_ioapic_routing(),
        APIC_END_OF_INTERRUPT_COUNT.load(Ordering::Acquire),
        LEGACY_END_OF_INTERRUPT_COUNT.load(Ordering::Acquire),
    )
}

/// Return legacy PIC boundary diagnostics.
pub fn get_legacy_pic_boundary_status() -> LegacyPicBoundaryStatus {
    LegacyPicBoundaryStatus::new(
        LEGACY_PIC_STATE_FLAGS.load(Ordering::Acquire),
        LEGACY_PIC_MASTER_MASK.load(Ordering::Acquire),
        LEGACY_PIC_SLAVE_MASK.load(Ordering::Acquire),
    )
}

fn should_mask_legacy_pic_for_apic_backend() -> bool {
    let provider = APIC_ROUTING_PROVIDER.lock();
    if !provider.configured || !has_local_apic() {
        return false;
    }

    provider.configuration.local_apic().is_enabled()
        && provider.configuration.local_apic().physical_address() != 0
        && provider.configuration.ioapic().physical_address() != 0
}

fn redirection_entry_for_legacy_irq(
    configuration: &ApicRoutingConfiguration,
    legacy_irq: u8,
    vector: u8,
    destination_apic_id: u32,
) -> Option<IoApicRedirectionEntry> {
    let ioapic = configuration.ioapic();
    let global_system_interrupt = configuration.global_system_interrupt_for_legacy_irq(legacy_irq);
    let table_index = global_system_interrupt.checked_sub(ioapic.global_system_interrupt_base())?;
    let low_register =
        IOAPIC_REDIRECTION_TABLE_BASE_REGISTER.checked_add(table_index.checked_mul(2)?)?;
    let flags = configuration
        .legacy_irq_route_for_irq(legacy_irq)
        .map_or(0, LegacyIrqRoute::flags);
    let mut low_value = u32::from(vector) & IOAPIC_REDIRECTION_VECTOR_MASK;
    if flags & ACPI_INTERRUPT_POLARITY_MASK == ACPI_INTERRUPT_ACTIVE_LOW {
        low_value |= IOAPIC_REDIRECTION_ACTIVE_LOW_BIT;
    }
    if flags & ACPI_INTERRUPT_TRIGGER_MASK == ACPI_INTERRUPT_LEVEL_TRIGGERED {
        low_value |= IOAPIC_REDIRECTION_LEVEL_TRIGGERED_BIT;
    }
    let high_value = (destination_apic_id & 0xff) << IOAPIC_DESTINATION_SHIFT;
    Some(IoApicRedirectionEntry::new(
        legacy_irq,
        global_system_interrupt,
        vector,
        table_index,
        low_register,
        low_value,
        high_value,
    ))
}

unsafe fn ensure_local_apic_software_enabled(registers: &LocalApicRegisters) -> bool {
    // SAFETY: The caller guarantees Local APIC MMIO is mapped and exclusively
    // programmed during interrupt-controller activation.
    let spurious_interrupt_vector =
        unsafe { registers.read(LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_REGISTER) };
    if spurious_interrupt_vector & LOCAL_APIC_SOFTWARE_ENABLE_BIT == 0 {
        // SAFETY: The caller guarantees Local APIC MMIO is mapped and
        // exclusively programmed during interrupt-controller activation.
        unsafe {
            registers.write(
                LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_REGISTER,
                spurious_interrupt_vector | LOCAL_APIC_SOFTWARE_ENABLE_BIT,
            );
        }
    }
    // SAFETY: The caller guarantees Local APIC MMIO is mapped and exclusively
    // programmed during interrupt-controller activation.
    let enabled_spurious_interrupt_vector =
        unsafe { registers.read(LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_REGISTER) };
    enabled_spurious_interrupt_vector & LOCAL_APIC_SOFTWARE_ENABLE_BIT != 0
}

unsafe fn activate_ioapic_redirection_plan(
    registers: &IoApicRegisters,
    plan: IoApicRedirectionPlan,
    status: &mut IoApicRoutingActivationStatus,
) {
    // SAFETY: The caller guarantees IOAPIC MMIO is mapped and exclusively
    // programmed during interrupt-controller activation.
    let version = unsafe { registers.read(IOAPIC_VERSION_REGISTER) };
    let maximum_redirection_entry = maximum_redirection_entry_from_version(version);

    let mut index = 0;
    while index < plan.entry_count() {
        let entry = plan
            .entry(index)
            .expect("retained IOAPIC redirection plan entry must exist");
        if entry.table_index() > maximum_redirection_entry {
            status.record_out_of_range_entry();
            index += 1;
            continue;
        }

        // SAFETY: The redirection entry was range-checked against the IOAPIC
        // version register, and the caller guarantees exclusive MMIO access.
        unsafe {
            activate_ioapic_redirection_entry(registers, entry, status);
        }
        index += 1;
    }
}

unsafe fn activate_ioapic_redirection_entry(
    registers: &IoApicRegisters,
    entry: IoApicRedirectionEntry,
    status: &mut IoApicRoutingActivationStatus,
) {
    let low_value = entry.low_value() & !IOAPIC_REDIRECTION_MASKED_BIT;
    let high_value = entry.high_value();
    // SAFETY: The caller guarantees the redirection registers are valid for
    // this IOAPIC and exclusively programmed during activation.
    unsafe {
        registers.write(entry.high_register(), high_value);
        registers.write(entry.low_register(), low_value);
    }
    // SAFETY: The redirection registers were just programmed through the mapped
    // IOAPIC window and can be read back for diagnostics.
    let high_readback = unsafe { registers.read(entry.high_register()) };
    // SAFETY: The redirection registers were just programmed through the mapped
    // IOAPIC window and can be read back for diagnostics.
    let low_readback = unsafe { registers.read(entry.low_register()) };
    status.record_activated_entry(entry, low_value, high_value, low_readback, high_readback);
}

fn mask_legacy_interrupts_for_apic_routing() {
    let mut interrupt_controllers = LEGACY_INTERRUPT_CONTROLLERS.lock();
    // SAFETY: Interrupts are disabled during IOAPIC routing activation, and
    // masking both PICs prevents legacy PIC delivery after IOAPIC routes become
    // active.
    unsafe {
        interrupt_controllers.write_masks(LEGACY_PIC_MASTER_APIC_MASK, LEGACY_PIC_SLAVE_APIC_MASK);
    }
    record_legacy_pic_boundary_status(
        LEGACY_PIC_MASKED_FOR_APIC_ROUTING_FLAG,
        LEGACY_PIC_MASTER_APIC_MASK,
        LEGACY_PIC_SLAVE_APIC_MASK,
    );
}

fn record_legacy_pic_boundary_status(flags: u8, master_mask: u8, slave_mask: u8) {
    LEGACY_PIC_MASTER_MASK.store(master_mask, Ordering::Release);
    LEGACY_PIC_SLAVE_MASK.store(slave_mask, Ordering::Release);
    LEGACY_PIC_STATE_FLAGS.store(flags, Ordering::Release);
}

struct LocalApicRegisters {
    base_address: usize,
}

impl LocalApicRegisters {
    const fn new(base_address: usize) -> Self {
        Self { base_address }
    }

    unsafe fn read(&self, register: usize) -> u32 {
        let register_pointer = self.register_pointer(register);
        // SAFETY: register_pointer points into mapped Local APIC MMIO space.
        // Volatile access is required for MMIO.
        unsafe { core::ptr::read_volatile(register_pointer) }
    }

    unsafe fn write(&self, register: usize, value: u32) {
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

struct IoApicRegisters {
    base_address: usize,
}

impl IoApicRegisters {
    fn new(physical_address: u64) -> Self {
        assert!(
            physical_address.is_multiple_of(4),
            "IOAPIC MMIO address must be 4-byte aligned"
        );
        Self {
            base_address: usize::try_from(physical_address)
                .expect("IOAPIC MMIO address must fit in usize"),
        }
    }

    unsafe fn read(&self, register: u32) -> u32 {
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

    unsafe fn write(&self, register: u32, value: u32) {
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

const fn maximum_redirection_entry_from_version(version: u32) -> u32 {
    (version >> IOAPIC_VERSION_MAX_REDIRECTION_ENTRY_SHIFT)
        & IOAPIC_VERSION_MAX_REDIRECTION_ENTRY_MASK
}

const fn local_apic_id_from_register(id_register: u32) -> u32 {
    (id_register >> LOCAL_APIC_ID_SHIFT) & LOCAL_APIC_ID_MASK
}

const fn maximum_lvt_entry_from_version(version: u32) -> u32 {
    (version >> LOCAL_APIC_VERSION_MAX_LVT_ENTRY_SHIFT) & LOCAL_APIC_VERSION_MAX_LVT_ENTRY_MASK
}

/// Initialize the legacy interrupt controller backend used by the current boot path.
///
/// # Safety
///
/// Must be called while interrupts are disabled.
pub unsafe fn initialize_legacy() {
    let mut interrupt_controllers = LEGACY_INTERRUPT_CONTROLLERS.lock();
    // SAFETY: The caller guarantees that interrupts are disabled while the
    // chained PICs are remapped and initialized.
    unsafe {
        interrupt_controllers.initialize();
    }

    // 0xf8: 11111000 (Timer, Keyboard, Cascade enabled)
    // 0xef: 11101111 (Mouse enabled)
    // SAFETY: The caller guarantees that interrupts are disabled while the
    // fallback PIC masks are written.
    unsafe {
        interrupt_controllers.write_masks(
            LEGACY_PIC_MASTER_FALLBACK_MASK,
            LEGACY_PIC_SLAVE_FALLBACK_MASK,
        );
    }
    record_legacy_pic_boundary_status(
        LEGACY_PIC_INITIALIZED_FLAG | LEGACY_PIC_FALLBACK_ENABLED_FLAG,
        LEGACY_PIC_MASTER_FALLBACK_MASK,
        LEGACY_PIC_SLAVE_FALLBACK_MASK,
    );
}

/// Notify the legacy interrupt controller that one interrupt has completed.
///
/// # Safety
///
/// Must be called exactly once after servicing a hardware interrupt delivered
/// by the legacy interrupt controller.
pub unsafe fn notify_legacy_end_of_interrupt(interrupt_index: u8) {
    LEGACY_INTERRUPT_CONTROLLERS
        .lock()
        .notify_end_of_interrupt(interrupt_index);
    LEGACY_END_OF_INTERRUPT_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Notify the configured interrupt controller that one interrupt completed.
///
/// # Safety
///
/// Must be called exactly once after servicing a hardware interrupt delivered
/// by the currently active interrupt controller backend.
pub unsafe fn notify_end_of_interrupt(interrupt_index: u8) {
    if has_ioapic_routing() {
        let base_address = LOCAL_APIC_EOI_BASE_ADDRESS.load(Ordering::Acquire);
        assert!(
            base_address != 0,
            "Local APIC EOI provider must be configured before APIC routing"
        );
        let registers = LocalApicRegisters::new(base_address);
        // SAFETY: APIC routing is active only after the Local APIC MMIO page has
        // been mapped and the EOI provider has been configured.
        unsafe {
            registers.write(LOCAL_APIC_EOI_REGISTER, LOCAL_APIC_EOI_VALUE);
        }
        APIC_END_OF_INTERRUPT_COUNT.fetch_add(1, Ordering::Relaxed);
    } else {
        // SAFETY: The legacy PIC backend remains active while IOAPIC routing is
        // disabled, so the interrupt must be acknowledged through the PIC.
        unsafe {
            notify_legacy_end_of_interrupt(interrupt_index);
        }
    }
}

/// Return whether Local APIC support is available on this CPU.
pub fn has_local_apic() -> bool {
    super::has_apic()
}

/// Return whether IOAPIC routing is available to use.
///
/// This becomes true only after the planned IOAPIC redirection entries are
/// unmasked, the legacy PIC is masked, and Local APIC EOI dispatch is active.
pub fn has_ioapic_routing() -> bool {
    IOAPIC_ROUTING_ACTIVE.load(Ordering::Acquire)
}
