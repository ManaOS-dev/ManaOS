use crate::serial_println;
use core::sync::atomic::{AtomicU64, Ordering};
use pic8259::ChainedPics;
use spin::Lazy;
use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

const INTERRUPT_CONTROLLER_1_OFFSET: u8 = 32;
const INTERRUPT_CONTROLLER_2_OFFSET: u8 = INTERRUPT_CONTROLLER_1_OFFSET + 8;

pub(super) static INTERRUPT_CONTROLLERS: Mutex<ChainedPics> =
    // SAFETY: The offsets reserve CPU exception vectors and match the configured
    // interrupt descriptor table entries.
    Mutex::new(unsafe {
        ChainedPics::new(INTERRUPT_CONTROLLER_1_OFFSET, INTERRUPT_CONTROLLER_2_OFFSET)
    });

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Return the number of timer ticks since interrupt initialization.
pub fn get_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = INTERRUPT_CONTROLLER_1_OFFSET,
    Keyboard,
    Mouse = INTERRUPT_CONTROLLER_1_OFFSET + 12,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

static INTERRUPT_DESCRIPTOR_TABLE: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
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

    table[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
    table[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
    table[InterruptIndex::Mouse.as_usize()].set_handler_fn(mouse_interrupt_handler);
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
    if let Some(mut interrupt_controllers) = INTERRUPT_CONTROLLERS.try_lock() {
        // SAFETY: End-of-interrupt is required after servicing the timer interrupt.
        unsafe {
            interrupt_controllers.notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
        }
    }
    crate::kernel::task::process_timer_tick();
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let status = status_read();
    if (status & 0x20) == 0 {
        // SAFETY: Port 0x60 is the PS/2 data port and status bit indicates keyboard data.
        let scancode = unsafe { Port::<u8>::new(0x60).read() };
        crate::kernel::driver::input::keyboard::push_scancode(scancode);
    }
    // SAFETY: Notify EOI to the PIC to allow future interrupts.
    unsafe {
        INTERRUPT_CONTROLLERS.lock().notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let status = status_read();
    if (status & 0x20) != 0 {
        // SAFETY: Port 0x60 is the PS/2 data port and status bit indicates mouse data.
        let packet = unsafe { Port::<u8>::new(0x60).read() };
        crate::kernel::driver::input::mouse::push_byte(packet);
    }
    // SAFETY: Notify EOI to the PIC to allow future interrupts.
    unsafe {
        INTERRUPT_CONTROLLERS.lock().notify_end_of_interrupt(InterruptIndex::Mouse.as_u8());
    }
}

fn status_read() -> u8 {
    // SAFETY: Port 0x64 is the PS/2 controller status port.
    unsafe { Port::<u8>::new(0x64).read() }
}
