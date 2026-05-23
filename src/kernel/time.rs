//! Kernel time source boundary.

use core::sync::atomic::{AtomicUsize, Ordering};

static TIMER_TICKS_PROVIDER: AtomicUsize = AtomicUsize::new(0);

/// Register the timer tick provider used by kernel subsystems.
pub fn register_timer_ticks_provider(provider: fn() -> u64) {
    TIMER_TICKS_PROVIDER.store(provider as usize, Ordering::Release);
}

/// Return the current timer tick count.
///
/// Returns zero until a provider is registered by the composition root.
pub fn get_timer_ticks() -> u64 {
    let provider = TIMER_TICKS_PROVIDER.load(Ordering::Acquire);
    if provider == 0 {
        return 0;
    }

    // SAFETY: register_timer_ticks_provider stores only valid fn() -> u64
    // pointers, and zero is handled above as the unregistered state.
    let provider: fn() -> u64 = unsafe { core::mem::transmute(provider) };
    provider()
}
