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

/// Check if the CPU supports APIC.
#[allow(dead_code)]
pub fn has_apic() -> bool {
    let cpuid = core::arch::x86_64::__cpuid(1);
    (cpuid.edx & (1 << 9)) != 0
}

/// `x86_64` specific implementations.
/// `x86_64` specific initialization.
pub fn init() {
    crate::serial_println!("[arch] Initializing GDT...");
    global_descriptor_table::init();
    crate::serial_println!("[arch] Initializing IDT...");
    interrupt_descriptor_table::initialize();
    crate::serial_println!(
        "[arch] Preferred interrupt controller: {:?}, IOAPIC routing: {}",
        interrupt_controller::get_preferred_kind(),
        interrupt_controller::has_ioapic_routing()
    );
    // SAFETY: The interrupt controllers are initialized while interrupts are
    // disabled during early architecture setup.
    unsafe {
        interrupt_controller::initialize_legacy();
    }

    // Initialize PIT (Programmable Interval Timer)
    crate::serial_println!(
        "[arch] Initializing PIT... preferred timer: {:?}, local APIC timer: {}",
        interval_timer::get_preferred_kind(),
        interval_timer::has_local_apic_timer()
    );
    interval_timer::initialize_programmable_interval_timer(1000);
}

/// Enable CPU interrupts after architecture and driver initialization.
pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
}

#[allow(dead_code)]
pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

core::arch::global_asm!(include_str!("context_switch.s"));

extern "C" {
    /// Switch from one saved task context to another.
    pub fn context_switch(current_context: *mut u64, next_context: *const u64);
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

/// Switch from one saved task context to another.
///
/// # Safety
///
/// This host-build stub is never used by the UEFI kernel runtime.
#[cfg(not(target_os = "uefi"))]
pub unsafe fn switch_context(_current_context: *mut u64, _next_context: *const u64) {}
