use core::arch::x86_64::_rdtsc;
use core::sync::atomic::{AtomicU64, Ordering};

static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);
const CALIBRATION_TICKS: u64 = 100;
const MAX_TIMER_WAIT_SPINS: u64 = 50_000_000;

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

fn wait_for_tick_change(start_ticks: u64) -> bool {
    for _ in 0..MAX_TIMER_WAIT_SPINS {
        if crate::kernel::time::get_timer_ticks() != start_ticks {
            return true;
        }
    }
    false
}

fn wait_until_tick_at_least(target_ticks: u64) -> bool {
    for _ in 0..MAX_TIMER_WAIT_SPINS {
        if crate::kernel::time::get_timer_ticks() >= target_ticks {
            return true;
        }
    }
    false
}

/// Calibrate TSC frequency using PIT.
/// This should be called after PIT is initialized and interrupts are enabled.
pub fn calibrate_tsc() {
    crate::kernel::task::set_preemption_enabled(false);

    let start_ticks = crate::kernel::time::get_timer_ticks();

    // Wait for the next tick to start measuring
    if !wait_for_tick_change(start_ticks) {
        crate::kernel::task::set_preemption_enabled(true);
        crate::serial_println!("[prof ] TSC calibration skipped: timer did not advance");
        return;
    }

    let tsc_start = read_tsc();
    let measure_start_ticks = crate::kernel::time::get_timer_ticks();
    let target_ticks = measure_start_ticks.saturating_add(CALIBRATION_TICKS);

    // Wait for 100ms (100 ticks at 1000Hz)
    if !wait_until_tick_at_least(target_ticks) {
        crate::kernel::task::set_preemption_enabled(true);
        crate::serial_println!("[prof ] TSC calibration skipped: timer wait timed out");
        return;
    }

    let tsc_end = read_tsc();
    let actual_ticks = crate::kernel::time::get_timer_ticks() - measure_start_ticks;
    if actual_ticks == 0 {
        crate::kernel::task::set_preemption_enabled(true);
        crate::serial_println!("[prof ] TSC calibration skipped: zero elapsed timer ticks");
        return;
    }

    // Calculate frequency (cycles per second)
    // (tsc_end - tsc_start) / (actual_ticks / 1000)
    let Some(elapsed_cycles) = tsc_end.checked_sub(tsc_start) else {
        crate::kernel::task::set_preemption_enabled(true);
        crate::serial_println!("[prof ] TSC calibration skipped: timestamp counter moved backward");
        return;
    };
    let Some(scaled_cycles) = elapsed_cycles.checked_mul(1000) else {
        crate::kernel::task::set_preemption_enabled(true);
        crate::serial_println!("[prof ] TSC calibration skipped: frequency calculation overflowed");
        return;
    };
    let freq = scaled_cycles / actual_ticks;
    TSC_FREQUENCY.store(freq, Ordering::Relaxed);

    crate::kernel::task::set_preemption_enabled(true);

    crate::serial_println!("[prof ] TSC Frequency calibrated: {} MHz", freq / 1_000_000);
}
