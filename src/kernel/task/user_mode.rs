//! User-mode transition metadata.

use core::sync::atomic::{AtomicU16, Ordering};

static USER_DATA_SELECTOR: AtomicU16 = AtomicU16::new(0);
static USER_CODE_SELECTOR: AtomicU16 = AtomicU16::new(0);

/// Selectors required for a future ring 3 transition.
#[derive(Debug, Clone, Copy)]
pub struct UserModeSelectors {
    /// User data segment selector value.
    pub data: u16,
    /// User code segment selector value.
    pub code: u16,
}

/// Register the user-mode segment selectors installed by the architecture.
pub fn register_selectors(selectors: UserModeSelectors) {
    USER_DATA_SELECTOR.store(selectors.data, Ordering::Release);
    USER_CODE_SELECTOR.store(selectors.code, Ordering::Release);
}

/// Return the user-mode segment selectors installed in the global descriptor table.
pub fn get_selectors() -> UserModeSelectors {
    UserModeSelectors {
        data: USER_DATA_SELECTOR.load(Ordering::Acquire),
        code: USER_CODE_SELECTOR.load(Ordering::Acquire),
    }
}
