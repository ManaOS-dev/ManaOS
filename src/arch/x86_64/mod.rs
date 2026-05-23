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
pub mod interrupt_descriptor_table;

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
    // SAFETY: The interrupt controllers are initialized while interrupts are
    // disabled during early architecture setup.
    unsafe {
        let mut interrupt_controllers = interrupt_descriptor_table::INTERRUPT_CONTROLLERS.lock();
        interrupt_controllers.initialize();

        // 0xf8: 11111000 (Timer, Keyboard, Cascade enabled)
        // 0xef: 11101111 (Mouse enabled)
        interrupt_controllers.write_masks(0xf8, 0xef);
    }

    // Initialize PIT (Programmable Interval Timer)
    crate::serial_println!("[arch] Initializing PIT...");
    init_pit(1000);
}

/// Enable CPU interrupts after architecture and driver initialization.
pub fn enable_interrupts() {
    x86_64::instructions::interrupts::enable();
}

/// Set PIT frequency to `target_hz`
fn init_pit(target_hz: u32) {
    use x86_64::instructions::port::Port;
    let divider = 1_193_182 / target_hz;
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
