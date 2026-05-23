use crate::serial_println;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::LazyLock;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

const INTERRUPT_CONTROLLER_1_OFFSET: u8 = 32;

static TICKS: AtomicU64 = AtomicU64::new(0);
static TIMER_TICK_PROCESSOR: AtomicUsize = AtomicUsize::new(0);
static KEYBOARD_BYTE_PROCESSOR: AtomicUsize = AtomicUsize::new(0);
static MOUSE_BYTE_PROCESSOR: AtomicUsize = AtomicUsize::new(0);

/// Kernel callbacks invoked by architecture interrupt handlers.
#[derive(Clone, Copy)]
pub struct InterruptProcessors {
    /// Called after each timer interrupt is acknowledged.
    pub timer_tick: fn(),
    /// Called with each keyboard byte received from hardware.
    pub keyboard_byte: fn(u8),
    /// Called with each mouse byte received from hardware.
    pub mouse_byte: fn(u8),
}

/// Return the number of timer ticks since interrupt initialization.
pub fn get_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Register kernel callbacks invoked by architecture interrupt handlers.
pub fn register_processors(processors: InterruptProcessors) {
    TIMER_TICK_PROCESSOR.store(processors.timer_tick as usize, Ordering::Release);
    KEYBOARD_BYTE_PROCESSOR.store(processors.keyboard_byte as usize, Ordering::Release);
    MOUSE_BYTE_PROCESSOR.store(processors.mouse_byte as usize, Ordering::Release);
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum InterruptIndex {
    Timer = INTERRUPT_CONTROLLER_1_OFFSET,
    Keyboard,
    Mouse = INTERRUPT_CONTROLLER_1_OFFSET + 12,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
}

static INTERRUPT_DESCRIPTOR_TABLE: LazyLock<InterruptDescriptorTable> = LazyLock::new(|| {
    let mut table = InterruptDescriptorTable::new();
    table.breakpoint.set_handler_fn(breakpoint_handler);
    // SAFETY: The double fault stack index is initialized in the global
    // descriptor table before interrupts are enabled.
    unsafe {
        table
            .double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::arch::x86_64::global_descriptor_table::DOUBLE_FAULT_IST_INDEX);
    }
    table.page_fault.set_handler_fn(page_fault_handler);
    table
        .general_protection_fault
        .set_handler_fn(general_protection_fault_handler);

    table[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
    table[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);
    table[InterruptIndex::Mouse.as_u8()].set_handler_fn(mouse_interrupt_handler);
    table
});

pub fn initialize() {
    INTERRUPT_DESCRIPTOR_TABLE.load();
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    panic!(
        "[EXCPT] PAGE FAULT\nAddr: {:?}\nCode: {:?}\n{:#?}",
        Cr2::read(),
        error_code,
        stack_frame
    );
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!("[EXCPT] GPF\nCode: {error_code:#x}\n{stack_frame:#?}");
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("[EXCPT] BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("[EXCPT] DOUBLE FAULT\n{stack_frame:#?}");
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);
    if let Some(mut interrupt_controllers) =
        crate::arch::x86_64::interrupt_controller::LEGACY_INTERRUPT_CONTROLLERS.try_lock()
    {
        // SAFETY: End-of-interrupt is required after servicing the timer interrupt.
        unsafe {
            interrupt_controllers.notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
        }
    }
    call_timer_tick_processor();
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let status = status_read();
    if (status & 0x20) == 0 {
        // SAFETY: Port 0x60 is the PS/2 data port and status bit indicates keyboard data.
        let scancode = unsafe { Port::<u8>::new(0x60).read() };
        call_keyboard_byte_processor(scancode);
    }
    // SAFETY: Notify EOI to the PIC to allow future interrupts.
    unsafe {
        crate::arch::x86_64::interrupt_controller::notify_legacy_end_of_interrupt(
            InterruptIndex::Keyboard.as_u8(),
        );
    }
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let status = status_read();
    if (status & 0x20) != 0 {
        // SAFETY: Port 0x60 is the PS/2 data port and status bit indicates mouse data.
        let packet = unsafe { Port::<u8>::new(0x60).read() };
        call_mouse_byte_processor(packet);
    }
    // SAFETY: Notify EOI to the PIC to allow future interrupts.
    unsafe {
        crate::arch::x86_64::interrupt_controller::notify_legacy_end_of_interrupt(
            InterruptIndex::Mouse.as_u8(),
        );
    }
}

fn status_read() -> u8 {
    // SAFETY: Port 0x64 is the PS/2 controller status port.
    unsafe { Port::<u8>::new(0x64).read() }
}

fn call_timer_tick_processor() {
    let processor = TIMER_TICK_PROCESSOR.load(Ordering::Acquire);
    if processor == 0 {
        return;
    }

    // SAFETY: register_processors stores only valid fn() pointers.
    let processor: fn() = unsafe { core::mem::transmute(processor) };
    processor();
}

fn call_keyboard_byte_processor(byte: u8) {
    let processor = KEYBOARD_BYTE_PROCESSOR.load(Ordering::Acquire);
    if processor == 0 {
        return;
    }

    // SAFETY: register_processors stores only valid fn(u8) pointers.
    let processor: fn(u8) = unsafe { core::mem::transmute(processor) };
    processor(byte);
}

fn call_mouse_byte_processor(byte: u8) {
    let processor = MOUSE_BYTE_PROCESSOR.load(Ordering::Acquire);
    if processor == 0 {
        return;
    }

    // SAFETY: register_processors stores only valid fn(u8) pointers.
    let processor: fn(u8) = unsafe { core::mem::transmute(processor) };
    processor(byte);
}
