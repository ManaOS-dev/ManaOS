//! Interrupt controller selection and initialization.

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
const LEGACY_TIMER_IRQ: u8 = 0;
const LEGACY_KEYBOARD_IRQ: u8 = 1;
const LEGACY_MOUSE_IRQ: u8 = 12;
const TIMER_INTERRUPT_VECTOR: u8 = INTERRUPT_CONTROLLER_1_OFFSET;
const KEYBOARD_INTERRUPT_VECTOR: u8 = INTERRUPT_CONTROLLER_1_OFFSET + 1;
const MOUSE_INTERRUPT_VECTOR: u8 = INTERRUPT_CONTROLLER_1_OFFSET + LEGACY_MOUSE_IRQ;
const IOAPIC_REDIRECTION_TABLE_BASE_REGISTER: u32 = 0x10;
const IOAPIC_REDIRECTION_VECTOR_MASK: u32 = 0xff;
const IOAPIC_REDIRECTION_ACTIVE_LOW_BIT: u32 = 1 << 13;
const IOAPIC_REDIRECTION_LEVEL_TRIGGERED_BIT: u32 = 1 << 15;
const IOAPIC_REDIRECTION_MASKED_BIT: u32 = 1 << 16;
const IOAPIC_DESTINATION_SHIFT: u32 = 24;
const ACPI_INTERRUPT_POLARITY_MASK: u16 = 0b11;
const ACPI_INTERRUPT_ACTIVE_LOW: u16 = 0b11;
const ACPI_INTERRUPT_TRIGGER_MASK: u16 = 0b11 << 2;
const ACPI_INTERRUPT_LEVEL_TRIGGERED: u16 = 0b11 << 2;

pub(super) static LEGACY_INTERRUPT_CONTROLLERS: Mutex<ChainedPics> =
    // SAFETY: The offsets reserve CPU exception vectors and match the configured
    // interrupt descriptor table entries.
    Mutex::new(unsafe {
        ChainedPics::new(INTERRUPT_CONTROLLER_1_OFFSET, INTERRUPT_CONTROLLER_2_OFFSET)
    });

static APIC_ROUTING_PROVIDER: Mutex<ApicRoutingProviderState> =
    Mutex::new(ApicRoutingProviderState::new());

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

/// Configure the architecture-owned APIC routing provider data.
pub fn configure_apic_routing_provider(configuration: &ApicRoutingConfiguration) {
    let mut provider = APIC_ROUTING_PROVIDER.lock();
    provider.configured = true;
    provider.configuration = *configuration;
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

/// Initialize the legacy interrupt controller backend used by the current boot path.
///
/// # Safety
///
/// Must be called while interrupts are disabled.
pub unsafe fn initialize_legacy() {
    let mut interrupt_controllers = LEGACY_INTERRUPT_CONTROLLERS.lock();
    interrupt_controllers.initialize();

    // 0xf8: 11111000 (Timer, Keyboard, Cascade enabled)
    // 0xef: 11101111 (Mouse enabled)
    interrupt_controllers.write_masks(0xf8, 0xef);
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
}

/// Return whether Local APIC support is available on this CPU.
pub fn has_local_apic() -> bool {
    super::has_apic()
}

/// Return whether IOAPIC routing is available to use.
///
/// `ManaOS` currently configures APIC routing provider data but still routes
/// interrupts through the legacy controllers until IOAPIC redirection entries
/// and EOI handling are wired.
pub fn has_ioapic_routing() -> bool {
    false
}
