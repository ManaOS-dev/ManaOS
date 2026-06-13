//! # `arch::x86_64`
//!
//! ## Owns
//! - `x86_64` descriptor table setup
//! - `x86_64` interrupt controller and timer setup
//! - CPU interrupt enablement
//! - Low-level context switching entry point
//!
//! ## Does NOT own
//! - Kernel scheduler policy
//! - Device driver state
//! - Input event queues
//!
//! ## Public API
//! - [`init`] - Initialize `x86_64` architecture state
//! - [`enable_interrupts`] - Enable CPU interrupts after wiring
//! - [`switch_context`] - Switch between saved task contexts

pub mod global_descriptor_table;
pub mod interrupt_controller;
pub mod interrupt_descriptor_table;
pub mod interval_timer;
pub mod pci_configuration;

const EXTENDED_FEATURE_ENABLE_REGISTER: u32 = 0xc000_0080;
const SYSTEM_CALL_TARGET_ADDRESS_REGISTER: u32 = 0xc000_0081;
const LONG_SYSTEM_CALL_TARGET_ADDRESS_REGISTER: u32 = 0xc000_0082;
const SYSTEM_CALL_FLAG_MASK_REGISTER: u32 = 0xc000_0084;
const SYSTEM_CALL_ENABLE_BIT: u64 = 1;
const INTERRUPT_FLAG_BIT: u64 = 1 << 9;
const KERNEL_CODE_SELECTOR: u16 = 0x08;

/// Check if the CPU supports APIC.
#[allow(dead_code)]
pub fn has_apic() -> bool {
    let cpuid = core::arch::x86_64::__cpuid(1);
    (cpuid.edx & (1 << 9)) != 0
}

/// `x86_64` specific initialization.
pub fn init(system_call_handler: u64) {
    crate::log_info!("arch", "Initializing GDT...");
    global_descriptor_table::init();
    crate::log_info!("arch", "Initializing IDT...");
    interrupt_descriptor_table::initialize();
    crate::log_info!("arch", "Initializing SYSCALL...");
    init_syscall(system_call_handler);
    crate::log_info!(
        "arch",
        "Preferred interrupt controller: {:?}, IOAPIC routing: {}",
        interrupt_controller::get_preferred_kind(),
        interrupt_controller::has_ioapic_routing()
    );
    // SAFETY: The interrupt controllers are initialized while interrupts are
    // disabled during early architecture setup.
    unsafe {
        interrupt_controller::initialize_legacy();
    }

    // Initialize PIT (Programmable Interval Timer)
    crate::log_info!(
        "arch",
        "Initializing PIT... preferred timer: {:?}, local APIC timer: {}",
        interval_timer::get_preferred_kind(),
        interval_timer::has_local_apic_timer()
    );
    interval_timer::initialize_programmable_interval_timer(1000);
}

/// Initialize the `x86_64` `SYSCALL`/`SYSRET` model-specific registers.
pub fn init_syscall(handler: u64) {
    use x86_64::registers::model_specific::Msr;

    let mut extended_feature_enable_register = Msr::new(EXTENDED_FEATURE_ENABLE_REGISTER);
    let mut system_call_target_address_register = Msr::new(SYSTEM_CALL_TARGET_ADDRESS_REGISTER);
    let mut long_system_call_target_address_register =
        Msr::new(LONG_SYSTEM_CALL_TARGET_ADDRESS_REGISTER);
    let mut system_call_flag_mask_register = Msr::new(SYSTEM_CALL_FLAG_MASK_REGISTER);

    // SAFETY: The EFER MSR exists on x86_64 CPUs and setting SCE enables the
    // architectural SYSCALL/SYSRET path.
    let extended_features = unsafe { extended_feature_enable_register.read() };
    // SAFETY: The written value preserves all existing EFER bits and enables SCE.
    unsafe {
        extended_feature_enable_register.write(extended_features | SYSTEM_CALL_ENABLE_BIT);
    }

    let user_system_return_selector = global_descriptor_table::USER_CODE_SELECTOR.wrapping_sub(16);
    let system_call_segments =
        (u64::from(user_system_return_selector) << 48) | (u64::from(KERNEL_CODE_SELECTOR) << 32);
    // SAFETY: STAR is the architectural segment selector MSR for
    // SYSCALL/SYSRET, and the selectors refer to entries loaded in the GDT.
    unsafe {
        system_call_target_address_register.write(system_call_segments);
    }
    // SAFETY: LSTAR is the architectural 64-bit syscall entry target MSR, and
    // `handler` is provided by the kernel composition root.
    unsafe {
        long_system_call_target_address_register.write(handler);
    }
    // SAFETY: SFMASK is the architectural syscall flags mask MSR; masking IF
    // disables interrupts on syscall entry.
    unsafe {
        system_call_flag_mask_register.write(INTERRUPT_FLAG_BIT);
    }
}

/// Enable CPU interrupts after architecture and driver initialization.
pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
}

/// Read the current `x86_64` timestamp counter value.
pub fn read_timestamp_counter() -> u64 {
    // SAFETY: RDTSC reads the processor timestamp counter and does not access
    // memory or require additional kernel invariants.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[allow(dead_code)]
pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

core::arch::global_asm!(include_str!("context_switch.s"));
core::arch::global_asm!(include_str!("interrupt_entry.s"));

extern "C" {
    /// Switch from one saved task context to another.
    pub fn context_switch(current_context: *mut u64, next_context: *const u64);
    /// Restore a user trap frame without returning to the caller.
    pub fn enter_user_mode(context: *const u64) -> !;
    /// Restore a user trap frame and return when the user task exits through `SYS_EXIT`.
    pub fn enter_user_mode_returnable(context: *const u64);
}

/// Switch from one saved task context to another.
///
/// # Safety
///
/// `current_context` and `next_context` must point to valid task context storage
/// with the layout expected by `context_switch.s`. The pointed tasks must remain
/// alive across the switch.
#[cfg(target_os = "uefi")]
pub unsafe fn switch_context(current_context: *mut u64, next_context: *const u64) {
    context_switch(current_context, next_context);
}

/// Restore a user trap frame and return when the user task exits through `SYS_EXIT`.
///
/// # Safety
///
/// `context` must point to a valid user trap frame whose code and
/// stack addresses are mapped as user-accessible pages.
#[cfg(target_os = "uefi")]
pub unsafe fn enter_user_mode_once(context: *const u64) {
    enter_user_mode_returnable(context);
}

/// Switch from one saved task context to another.
///
/// # Safety
///
/// This host-build stub is never used by the UEFI kernel runtime.
#[cfg(not(target_os = "uefi"))]
pub unsafe fn switch_context(_current_context: *mut u64, _next_context: *const u64) {}

/// Restore a user trap frame and return when the user task exits through `SYS_EXIT`.
///
/// # Safety
///
/// This host-build stub is never used by the UEFI kernel runtime.
#[cfg(not(target_os = "uefi"))]
pub unsafe fn enter_user_mode_once(_context: *const u64) {}
