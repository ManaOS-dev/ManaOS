//! Interrupt controller selection and initialization.

use pic8259::ChainedPics;
use spin::Mutex;

const INTERRUPT_CONTROLLER_1_OFFSET: u8 = 32;
const INTERRUPT_CONTROLLER_2_OFFSET: u8 = INTERRUPT_CONTROLLER_1_OFFSET + 8;

pub(super) static LEGACY_INTERRUPT_CONTROLLERS: Mutex<ChainedPics> =
    // SAFETY: The offsets reserve CPU exception vectors and match the configured
    // interrupt descriptor table entries.
    Mutex::new(unsafe {
        ChainedPics::new(INTERRUPT_CONTROLLER_1_OFFSET, INTERRUPT_CONTROLLER_2_OFFSET)
    });

/// Available interrupt controller backends.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InterruptControllerKind {
    /// Legacy 8259 chained interrupt controllers.
    Legacy8259,
    /// Local APIC plus IOAPIC-capable hardware.
    LocalApicIoApic,
}

/// Return the interrupt controller backend preferred by CPU capability.
pub fn get_preferred_kind() -> InterruptControllerKind {
    if super::has_apic() {
        InterruptControllerKind::LocalApicIoApic
    } else {
        InterruptControllerKind::Legacy8259
    }
}

/// Initialize the legacy interrupt controller backend used by the current boot path.
///
/// # Safety
///
/// Must be called while interrupts are disabled.
pub unsafe fn initialize_legacy() {
    let mut interrupt_controllers = LEGACY_INTERRUPT_CONTROLLERS.lock();
    interrupt_controllers.initialize();

    // 0xf8: 11111000 (Timer, Keyboard, Cascade enabled)
    // 0xef: 11101111 (Mouse enabled)
    interrupt_controllers.write_masks(0xf8, 0xef);
}

/// Notify the legacy interrupt controller that one interrupt has completed.
///
/// # Safety
///
/// Must be called exactly once after servicing a hardware interrupt delivered
/// by the legacy interrupt controller.
pub unsafe fn notify_legacy_end_of_interrupt(interrupt_index: u8) {
    LEGACY_INTERRUPT_CONTROLLERS
        .lock()
        .notify_end_of_interrupt(interrupt_index);
}

/// Return whether Local APIC support is available on this CPU.
pub fn has_local_apic() -> bool {
    super::has_apic()
}

/// Return whether IOAPIC routing is available to use.
///
/// `ManaOS` currently detects APIC-capable CPUs but still routes interrupts
/// through the legacy controllers until ACPI MADT parsing is added.
pub fn has_ioapic_routing() -> bool {
    false
}
