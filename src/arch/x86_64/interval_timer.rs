//! Interval timer selection and initialization.

/// Available interval timer backends.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IntervalTimerKind {
    /// Legacy programmable interval timer.
    ProgrammableIntervalTimer,
    /// Local APIC timer.
    LocalApicTimer,
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

/// Return whether the Local APIC timer can be selected after APIC routing setup.
pub fn has_local_apic_timer() -> bool {
    super::interrupt_controller::has_local_apic()
}
