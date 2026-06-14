//! Interval timer selection and initialization.

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

const INTERRUPT_CONTROLLER_1_OFFSET: u8 = 32;
const LOCAL_APIC_LVT_TIMER_REGISTER: usize = 0x320;
const LOCAL_APIC_TIMER_INITIAL_COUNT_REGISTER: usize = 0x380;
const LOCAL_APIC_TIMER_CURRENT_COUNT_REGISTER: usize = 0x390;
const LOCAL_APIC_TIMER_DIVIDE_CONFIGURATION_REGISTER: usize = 0x3e0;
const LOCAL_APIC_LVT_TIMER_MASKED_BIT: u32 = 1 << 16;
const LOCAL_APIC_TIMER_DIVIDE_BY_16_VALUE: u32 = 0b0011;
const LOCAL_APIC_TIMER_DIVIDE_BY_16_DENOMINATOR: u32 = 16;
const LOCAL_APIC_TIMER_CALIBRATION_INITIAL_COUNT: u32 = u32::MAX;
const LOCAL_APIC_TIMER_CALIBRATION_CONFIGURED_FLAG: u8 = 1;
const LOCAL_APIC_TIMER_CALIBRATION_ARMED_FLAG: u8 = 1 << 1;
const LOCAL_APIC_TIMER_CALIBRATION_MASKED_FLAG: u8 = 1 << 2;
const LOCAL_APIC_TIMER_CALIBRATION_DECREMENTED_FLAG: u8 = 1 << 3;
const LOCAL_APIC_TIMER_CALIBRATION_EXPIRED_FLAG: u8 = 1 << 4;

static LOCAL_APIC_TIMER_CALIBRATION_BASE_ADDRESS: AtomicUsize = AtomicUsize::new(0);
static LOCAL_APIC_TIMER_CALIBRATION_START_TICKS: AtomicU64 = AtomicU64::new(0);
static LOCAL_APIC_TIMER_CALIBRATION_INITIAL_COUNT_READBACK: AtomicU32 = AtomicU32::new(0);

/// Available interval timer backends.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IntervalTimerKind {
    /// Legacy programmable interval timer.
    ProgrammableIntervalTimer,
    /// Local APIC timer.
    LocalApicTimer,
}

/// Masked Local APIC timer calibration diagnostics.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LocalApicTimerCalibrationStatus {
    flags: u8,
    physical_address: u64,
    start_ticks: u64,
    current_ticks: u64,
    lvt_timer: u32,
    divide_configuration: u32,
    divide_denominator: u32,
    initial_count: u32,
    current_count: u32,
}

impl LocalApicTimerCalibrationStatus {
    /// Return whether a Local APIC timer calibration sample is configured.
    pub const fn is_configured(self) -> bool {
        self.flags & LOCAL_APIC_TIMER_CALIBRATION_CONFIGURED_FLAG != 0
    }

    /// Return whether the Local APIC timer sample has been armed.
    pub const fn is_armed(self) -> bool {
        self.flags & LOCAL_APIC_TIMER_CALIBRATION_ARMED_FLAG != 0
    }

    /// Return whether the Local APIC timer interrupt is masked.
    pub const fn is_masked(self) -> bool {
        self.flags & LOCAL_APIC_TIMER_CALIBRATION_MASKED_FLAG != 0
    }

    /// Return whether the Local APIC timer current count has decreased.
    pub const fn has_decremented(self) -> bool {
        self.flags & LOCAL_APIC_TIMER_CALIBRATION_DECREMENTED_FLAG != 0
    }

    /// Return whether the Local APIC timer sample expired before inspection.
    pub const fn has_expired(self) -> bool {
        self.flags & LOCAL_APIC_TIMER_CALIBRATION_EXPIRED_FLAG != 0
    }

    /// Return the Local APIC MMIO physical address used by the sample.
    pub const fn physical_address(self) -> u64 {
        self.physical_address
    }

    /// Return the PIT tick value captured when the sample was armed.
    pub const fn start_ticks(self) -> u64 {
        self.start_ticks
    }

    /// Return the PIT tick value captured when the sample was inspected.
    pub const fn current_ticks(self) -> u64 {
        self.current_ticks
    }

    /// Return the PIT ticks elapsed since the sample was armed.
    pub const fn elapsed_ticks(self) -> u64 {
        self.current_ticks.saturating_sub(self.start_ticks)
    }

    /// Return the raw Local APIC LVT timer register.
    pub const fn lvt_timer(self) -> u32 {
        self.lvt_timer
    }

    /// Return the interrupt vector programmed into the LVT timer register.
    pub const fn vector(self) -> u8 {
        (self.lvt_timer & 0xff) as u8
    }

    /// Return the raw Local APIC timer divide configuration register.
    pub const fn divide_configuration(self) -> u32 {
        self.divide_configuration
    }

    /// Return the timer divide denominator represented by the configuration.
    pub const fn divide_denominator(self) -> u32 {
        self.divide_denominator
    }

    /// Return the initial Local APIC timer count read back after arming.
    pub const fn initial_count(self) -> u32 {
        self.initial_count
    }

    /// Return the Local APIC timer current count.
    pub const fn current_count(self) -> u32 {
        self.current_count
    }

    /// Return the Local APIC timer counts elapsed since arming.
    pub const fn elapsed_counts(self) -> u32 {
        self.initial_count.saturating_sub(self.current_count)
    }

    /// Return the observed Local APIC timer counts per PIT tick.
    pub fn counts_per_tick(self) -> u64 {
        u64::from(self.elapsed_counts())
            .checked_div(self.elapsed_ticks())
            .unwrap_or(0)
    }
}

/// Return the timer backend preferred by CPU capability.
pub fn get_preferred_kind() -> IntervalTimerKind {
    if super::interrupt_controller::has_local_apic() {
        IntervalTimerKind::LocalApicTimer
    } else {
        IntervalTimerKind::ProgrammableIntervalTimer
    }
}

/// Initialize the legacy programmable interval timer.
///
/// # Panics
///
/// Panics if `target_hertz` is zero.
pub fn initialize_programmable_interval_timer(target_hertz: u64) {
    use x86_64::instructions::port::Port;

    assert!(target_hertz != 0, "PIT target frequency must be non-zero");
    let divider = 1_193_182_u64 / target_hertz;
    let mut command_port = Port::<u8>::new(0x43);
    let mut data_port = Port::<u8>::new(0x40);

    // SAFETY: Ports 0x43 and 0x40 are the interval timer command and channel 0
    // data ports, used during single-threaded architecture initialization.
    unsafe {
        command_port.write(0x36); // Square wave, Lo/Hi byte
        data_port.write((divider & 0xFF) as u8);
        data_port.write(((divider >> 8) & 0xFF) as u8);
    }
}

/// Start a masked Local APIC timer calibration sample.
///
/// The sample keeps the Local APIC timer interrupt masked while the current
/// count runs down. A later inspection can compare the count delta against the
/// still-active PIT tick counter before `ManaOS` switches scheduling ticks to the
/// Local APIC timer.
///
/// # Safety
///
/// The Local APIC MMIO page reported by the APIC routing provider must be
/// identity-mapped as writable kernel memory, and no other code may program the
/// Local APIC timer registers concurrently.
pub unsafe fn start_masked_local_apic_timer_calibration(
    start_ticks: u64,
) -> Option<LocalApicTimerCalibrationStatus> {
    let base_address = local_apic_timer_base_address()?;
    let registers = LocalApicTimerRegisters::new(base_address);
    // SAFETY: The caller guarantees that the Local APIC MMIO page is mapped and
    // exclusively available for timer calibration setup.
    unsafe {
        registers.write(LOCAL_APIC_TIMER_INITIAL_COUNT_REGISTER, 0);
        registers.write(
            LOCAL_APIC_TIMER_DIVIDE_CONFIGURATION_REGISTER,
            LOCAL_APIC_TIMER_DIVIDE_BY_16_VALUE,
        );
        registers.write(
            LOCAL_APIC_LVT_TIMER_REGISTER,
            u32::from(INTERRUPT_CONTROLLER_1_OFFSET) | LOCAL_APIC_LVT_TIMER_MASKED_BIT,
        );
        registers.write(
            LOCAL_APIC_TIMER_INITIAL_COUNT_REGISTER,
            LOCAL_APIC_TIMER_CALIBRATION_INITIAL_COUNT,
        );
    }
    // SAFETY: The same mapped Local APIC timer initial-count register was just
    // programmed and can be read back for diagnostics.
    let initial_count_readback = unsafe { registers.read(LOCAL_APIC_TIMER_INITIAL_COUNT_REGISTER) };
    LOCAL_APIC_TIMER_CALIBRATION_BASE_ADDRESS.store(base_address, Ordering::Release);
    LOCAL_APIC_TIMER_CALIBRATION_START_TICKS.store(start_ticks, Ordering::Release);
    LOCAL_APIC_TIMER_CALIBRATION_INITIAL_COUNT_READBACK
        .store(initial_count_readback, Ordering::Release);
    // SAFETY: The timer was just armed through the same mapped register page.
    unsafe { inspect_masked_local_apic_timer_calibration(start_ticks) }
}

/// Inspect the masked Local APIC timer calibration sample.
///
/// # Safety
///
/// The Local APIC MMIO page captured by
/// [`start_masked_local_apic_timer_calibration`] must remain mapped as readable
/// kernel memory.
pub unsafe fn inspect_masked_local_apic_timer_calibration(
    current_ticks: u64,
) -> Option<LocalApicTimerCalibrationStatus> {
    let base_address = LOCAL_APIC_TIMER_CALIBRATION_BASE_ADDRESS.load(Ordering::Acquire);
    if base_address == 0 {
        return None;
    }

    let registers = LocalApicTimerRegisters::new(base_address);
    // SAFETY: The caller guarantees that the Local APIC MMIO page remains
    // mapped and readable for calibration diagnostics.
    let lvt_timer = unsafe { registers.read(LOCAL_APIC_LVT_TIMER_REGISTER) };
    // SAFETY: The caller guarantees that the Local APIC MMIO page remains
    // mapped and readable for calibration diagnostics.
    let divide_configuration =
        unsafe { registers.read(LOCAL_APIC_TIMER_DIVIDE_CONFIGURATION_REGISTER) };
    // SAFETY: The caller guarantees that the Local APIC MMIO page remains
    // mapped and readable for calibration diagnostics.
    let current_count = unsafe { registers.read(LOCAL_APIC_TIMER_CURRENT_COUNT_REGISTER) };
    let initial_count = LOCAL_APIC_TIMER_CALIBRATION_INITIAL_COUNT_READBACK.load(Ordering::Acquire);
    let start_ticks = LOCAL_APIC_TIMER_CALIBRATION_START_TICKS.load(Ordering::Acquire);
    let mut flags = LOCAL_APIC_TIMER_CALIBRATION_CONFIGURED_FLAG;
    if initial_count != 0 {
        flags |= LOCAL_APIC_TIMER_CALIBRATION_ARMED_FLAG;
    }
    if lvt_timer & LOCAL_APIC_LVT_TIMER_MASKED_BIT != 0 {
        flags |= LOCAL_APIC_TIMER_CALIBRATION_MASKED_FLAG;
    }
    if current_count < initial_count {
        flags |= LOCAL_APIC_TIMER_CALIBRATION_DECREMENTED_FLAG;
    }
    if current_count == 0 && initial_count != 0 {
        flags |= LOCAL_APIC_TIMER_CALIBRATION_EXPIRED_FLAG;
    }

    Some(LocalApicTimerCalibrationStatus {
        flags,
        physical_address: u64::try_from(base_address)
            .expect("Local APIC timer MMIO address must fit in u64"),
        start_ticks,
        current_ticks,
        lvt_timer,
        divide_configuration,
        divide_denominator: LOCAL_APIC_TIMER_DIVIDE_BY_16_DENOMINATOR,
        initial_count,
        current_count,
    })
}

/// Return whether the Local APIC timer can be selected after APIC routing setup.
pub fn has_local_apic_timer() -> bool {
    super::interrupt_controller::has_local_apic()
}

fn local_apic_timer_base_address() -> Option<usize> {
    let status = super::interrupt_controller::get_apic_routing_provider_status();
    let local_apic = status.local_apic();
    if !status.is_configured() || !local_apic.is_enabled() || local_apic.physical_address() == 0 {
        return None;
    }

    usize::try_from(local_apic.physical_address()).ok()
}

struct LocalApicTimerRegisters {
    base_address: usize,
}

impl LocalApicTimerRegisters {
    const fn new(base_address: usize) -> Self {
        Self { base_address }
    }

    unsafe fn read(&self, register: usize) -> u32 {
        let register_pointer = self.register_pointer(register);
        // SAFETY: register_pointer points into mapped Local APIC timer MMIO
        // space. Volatile access is required for MMIO.
        unsafe { core::ptr::read_volatile(register_pointer) }
    }

    unsafe fn write(&self, register: usize, value: u32) {
        let register_pointer = self.register_pointer(register);
        // SAFETY: register_pointer points into mapped Local APIC timer MMIO
        // space. Volatile access is required for MMIO.
        unsafe {
            core::ptr::write_volatile(register_pointer, value);
        }
    }

    fn register_pointer(&self, register: usize) -> *mut u32 {
        self.base_address
            .checked_add(register)
            .expect("Local APIC timer register address overflowed") as *mut u32
    }
}
