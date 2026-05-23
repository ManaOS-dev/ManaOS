use core::arch::x86_64::_rdtsc;
use core::sync::atomic::{AtomicU64, Ordering};

static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);

/// Return the calibrated timestamp counter frequency in hertz.
#[allow(dead_code)]
pub fn get_tsc_frequency() -> u64 {
    TSC_FREQUENCY.load(Ordering::Relaxed)
}

/// Reads the current TSC value.
pub fn read_tsc() -> u64 {
    // SAFETY: rdtsc is a standard x86 instruction and safe to call.
    unsafe { _rdtsc() }
}

/// Calibrate TSC frequency using PIT.
/// This should be called after PIT is initialized and interrupts are enabled.
pub fn calibrate_tsc() {
    crate::kernel::task::set_preemption_enabled(false);

    let start_ticks = crate::kernel::time::get_timer_ticks();

    // Wait for the next tick to start measuring
    while crate::kernel::time::get_timer_ticks() == start_ticks {}

    let tsc_start = read_tsc();
    let measure_start_ticks = crate::kernel::time::get_timer_ticks();

    // Wait for 100ms (100 ticks at 1000Hz)
    while crate::kernel::time::get_timer_ticks() < measure_start_ticks + 100 {}

    let tsc_end = read_tsc();
    let actual_ticks = crate::kernel::time::get_timer_ticks() - measure_start_ticks;

    // Calculate frequency (cycles per second)
    // (tsc_end - tsc_start) / (actual_ticks / 1000)
    let freq = (tsc_end - tsc_start) * 1000 / actual_ticks;
    TSC_FREQUENCY.store(freq, Ordering::Relaxed);

    crate::kernel::task::set_preemption_enabled(true);

    crate::serial_println!("[prof ] TSC Frequency calibrated: {} MHz", freq / 1_000_000);
}
