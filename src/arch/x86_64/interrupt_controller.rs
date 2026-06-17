//! Interrupt controller selection and initialization.

use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
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
const LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_MASK: u32 = 0xff;
const LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_NUMBER: u32 = 0xff;
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

mod registers;
use registers::{IoApicRegisters, LocalApicRegisters};

mod configuration;

pub use configuration::{
    ApicMmioAddress, ApicRoutingConfiguration, ApicRoutingProviderStatus, EndOfInterruptStatus,
    InterruptControllerKind, IoApicConfiguration, IoApicRedirectionEntry, IoApicRedirectionPlan,
    IoApicRedirectionStagingStatus, IoApicRoutingActivationStatus, IoApicTimerRouteMaskStatus,
    LegacyIrqRoute, LegacyPicBoundaryStatus, LocalApicConfiguration, LocalApicEoiProviderStatus,
};

static LEGACY_INTERRUPT_CONTROLLERS: Mutex<ChainedPics> =
    // SAFETY: The offsets reserve CPU exception vectors and match the configured
    // interrupt descriptor table entries.
    Mutex::new(unsafe {
        ChainedPics::new(INTERRUPT_CONTROLLER_1_OFFSET, INTERRUPT_CONTROLLER_2_OFFSET)
    });

static APIC_ROUTING_PROVIDER: Mutex<ApicRoutingProviderState> =
    Mutex::new(ApicRoutingProviderState::new());
static LOCAL_APIC_EOI_BASE_ADDRESS: AtomicU64 = AtomicU64::new(0);
static IOAPIC_ROUTING_ACTIVE: AtomicBool = AtomicBool::new(false);
static APIC_END_OF_INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);
static LEGACY_END_OF_INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);
static LEGACY_PIC_STATE_FLAGS: AtomicU8 = AtomicU8::new(0);
static LEGACY_PIC_MASTER_MASK: AtomicU8 = AtomicU8::new(LEGACY_PIC_MASTER_APIC_MASK);
static LEGACY_PIC_SLAVE_MASK: AtomicU8 = AtomicU8::new(LEGACY_PIC_SLAVE_APIC_MASK);

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
    let local_apic_address = configuration.local_apic().physical_address().as_u64();
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
    let raw_base_address = LOCAL_APIC_EOI_BASE_ADDRESS.load(Ordering::Acquire);
    if raw_base_address == 0 {
        return None;
    }

    let base_address = ApicMmioAddress::new(raw_base_address);
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
        base_address.as_u64(),
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
    let raw_local_apic_base_address = LOCAL_APIC_EOI_BASE_ADDRESS.load(Ordering::Acquire);
    if raw_local_apic_base_address == 0 {
        return None;
    }

    let local_apic_base_address = ApicMmioAddress::new(raw_local_apic_base_address);
    let local_apic_registers = LocalApicRegisters::new(local_apic_base_address);
    let ioapic_registers = IoApicRegisters::new(configuration.ioapic().physical_address());
    let mut status = IoApicRoutingActivationStatus::new(plan.entry_count());

    // SAFETY: The caller guarantees Local APIC MMIO is mapped and exclusively
    // programmed during interrupt-controller activation.
    if unsafe { ensure_local_apic_spurious_vector_enabled(&local_apic_registers) } {
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
        && !provider
            .configuration
            .local_apic()
            .physical_address()
            .is_zero()
        && !provider.configuration.ioapic().physical_address().is_zero()
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

unsafe fn ensure_local_apic_spurious_vector_enabled(registers: &LocalApicRegisters) -> bool {
    // SAFETY: The caller guarantees Local APIC MMIO is mapped and exclusively
    // programmed during interrupt-controller activation.
    let spurious_interrupt_vector =
        unsafe { registers.read(LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_REGISTER) };
    let configured_spurious_interrupt_vector = (spurious_interrupt_vector
        & !LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_MASK)
        | LOCAL_APIC_SOFTWARE_ENABLE_BIT
        | LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_NUMBER;
    if spurious_interrupt_vector != configured_spurious_interrupt_vector {
        // SAFETY: The caller guarantees Local APIC MMIO is mapped and
        // exclusively programmed during interrupt-controller activation.
        unsafe {
            registers.write(
                LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_REGISTER,
                configured_spurious_interrupt_vector,
            );
        }
    }
    // SAFETY: The caller guarantees Local APIC MMIO is mapped and exclusively
    // programmed during interrupt-controller activation.
    let enabled_spurious_interrupt_vector =
        unsafe { registers.read(LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_REGISTER) };
    enabled_spurious_interrupt_vector & LOCAL_APIC_SOFTWARE_ENABLE_BIT != 0
        && enabled_spurious_interrupt_vector & LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_MASK
            == LOCAL_APIC_SPURIOUS_INTERRUPT_VECTOR_NUMBER
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
        // SAFETY: APIC routing is active only after the Local APIC MMIO page
        // has been mapped and the EOI provider has been configured.
        unsafe {
            notify_local_apic_end_of_interrupt();
        }
    } else {
        // SAFETY: The legacy PIC backend remains active while IOAPIC routing is
        // disabled, so the interrupt must be acknowledged through the PIC.
        unsafe {
            notify_legacy_end_of_interrupt(interrupt_index);
        }
    }
}

/// Notify APIC EOI for an unexpected external interrupt without a routed vector.
///
/// # Safety
///
/// Must be called only after servicing an unexpected external interrupt handler
/// entry. The handler must not use this for Local APIC spurious interrupts,
/// which do not require EOI.
pub unsafe fn notify_unexpected_external_end_of_interrupt() {
    if has_ioapic_routing() {
        // SAFETY: APIC routing is active only after the Local APIC MMIO page
        // has been mapped and the EOI provider has been configured.
        unsafe {
            notify_local_apic_end_of_interrupt();
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

unsafe fn notify_local_apic_end_of_interrupt() {
    let raw_base_address = LOCAL_APIC_EOI_BASE_ADDRESS.load(Ordering::Acquire);
    assert!(
        raw_base_address != 0,
        "Local APIC EOI provider must be configured before APIC routing"
    );
    let base_address = ApicMmioAddress::new(raw_base_address);
    let registers = LocalApicRegisters::new(base_address);
    // SAFETY: APIC routing is active only after the Local APIC MMIO page has
    // been mapped and the EOI provider has been configured.
    unsafe {
        registers.write(LOCAL_APIC_EOI_REGISTER, LOCAL_APIC_EOI_VALUE);
    }
    APIC_END_OF_INTERRUPT_COUNT.fetch_add(1, Ordering::Relaxed);
}
