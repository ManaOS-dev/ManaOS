use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);
static TIMESTAMP_COUNTER_PROVIDER: AtomicUsize = AtomicUsize::new(0);

type TimestampCounterProvider = fn() -> u64;
const CALIBRATION_TICKS: u64 = 100;
const MAX_TIMER_WAIT_SPINS: u64 = 50_000_000;
const MAX_CALIBRATION_ATTEMPTS: usize = 5;
const TIMER_TICKS_PER_SECOND: u64 = 1000;
const HERTZ_PER_MEGAHERTZ: u64 = 1_000_000;
const MIN_REASONABLE_TSC_FREQUENCY: u64 = 100 * HERTZ_PER_MEGAHERTZ;
const MAX_REASONABLE_TSC_FREQUENCY: u64 = 10_000 * HERTZ_PER_MEGAHERTZ;

/// Return the calibrated timestamp counter frequency in hertz.
#[allow(dead_code)]
pub fn get_tsc_frequency() -> u64 {
    TSC_FREQUENCY.load(Ordering::Relaxed)
}

/// Register the platform timestamp counter reader used by the profiler.
pub fn register_timestamp_counter_provider(provider: TimestampCounterProvider) {
    TIMESTAMP_COUNTER_PROVIDER.store(provider as usize, Ordering::Relaxed);
}

/// Reads the current TSC value.
pub fn read_tsc() -> u64 {
    let provider_address = TIMESTAMP_COUNTER_PROVIDER.load(Ordering::Relaxed);
    if provider_address == 0 {
        return 0;
    }

    // SAFETY: The stored address is written only by
    // `register_timestamp_counter_provider` from a valid function pointer.
    let provider: TimestampCounterProvider = unsafe { core::mem::transmute(provider_address) };
    provider()
}

fn wait_for_tick_change_and_read_tsc(start_ticks: u64) -> Option<(u64, u64)> {
    for _ in 0..MAX_TIMER_WAIT_SPINS {
        let current_ticks = crate::kernel::time::get_timer_ticks();
        if current_ticks != start_ticks {
            return Some((current_ticks, read_tsc()));
        }
    }
    None
}

fn wait_until_tick_at_least_and_read_tsc(target_ticks: u64) -> Option<(u64, u64)> {
    for _ in 0..MAX_TIMER_WAIT_SPINS {
        let current_ticks = crate::kernel::time::get_timer_ticks();
        if current_ticks >= target_ticks {
            return Some((current_ticks, read_tsc()));
        }
    }
    None
}

fn measure_tsc_frequency() -> Option<u64> {
    let start_ticks = crate::kernel::time::get_timer_ticks();

    // Wait for the next tick to start measuring
    let (measure_start_ticks, tsc_start) = wait_for_tick_change_and_read_tsc(start_ticks)?;

    let target_ticks = measure_start_ticks.saturating_add(CALIBRATION_TICKS);

    // Wait for 100ms (100 ticks at 1000Hz)
    let (measure_end_ticks, tsc_end) = wait_until_tick_at_least_and_read_tsc(target_ticks)?;

    let actual_ticks = measure_end_ticks.saturating_sub(measure_start_ticks);
    if actual_ticks == 0 {
        return None;
    }

    // Calculate frequency (cycles per second)
    // (tsc_end - tsc_start) / (actual_ticks / 1000)
    let elapsed_cycles = tsc_end.checked_sub(tsc_start)?;
    let scaled_cycles = elapsed_cycles.checked_mul(TIMER_TICKS_PER_SECOND)?;
    Some(scaled_cycles / actual_ticks)
}

/// Calibrate TSC frequency using PIT.
/// This should be called after PIT is initialized and interrupts are enabled.
pub fn calibrate_tsc() {
    crate::kernel::task::set_preemption_enabled(false);

    let mut frequency = None;
    for _ in 0..MAX_CALIBRATION_ATTEMPTS {
        let Some(candidate) = measure_tsc_frequency() else {
            continue;
        };

        if (MIN_REASONABLE_TSC_FREQUENCY..=MAX_REASONABLE_TSC_FREQUENCY).contains(&candidate) {
            frequency = Some(candidate);
            break;
        }
    }

    let Some(frequency) = frequency else {
        crate::kernel::task::set_preemption_enabled(true);
        crate::log_warn!(
            "profiler",
            "TSC calibration skipped: no stable timer sample"
        );
        return;
    };

    TSC_FREQUENCY.store(frequency, Ordering::Relaxed);

    crate::kernel::task::set_preemption_enabled(true);

    crate::log_info!(
        "profiler",
        "TSC frequency calibrated: {} MHz",
        frequency / HERTZ_PER_MEGAHERTZ
    );
}
