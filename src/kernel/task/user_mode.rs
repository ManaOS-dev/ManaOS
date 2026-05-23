//! User-mode transition metadata.

/// Selectors required for a future ring 3 transition.
#[derive(Debug, Clone, Copy)]
pub struct UserModeSelectors {
    /// User data segment selector value.
    pub data: u16,
    /// User code segment selector value.
    pub code: u16,
}

/// Return the user-mode segment selectors installed in the global descriptor table.
pub fn get_selectors() -> UserModeSelectors {
    UserModeSelectors {
        data: crate::arch::x86_64::global_descriptor_table::get_user_data_selector().0,
        code: crate::arch::x86_64::global_descriptor_table::get_user_code_selector().0,
    }
}
