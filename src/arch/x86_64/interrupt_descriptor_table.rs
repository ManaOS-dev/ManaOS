use crate::serial_println;
use crate::shared::{
    PageFaultAddress, PageFaultErrorBits, PageFaultInstructionPointer, PageFaultReport,
    TimerInterruptFrame,
};
use core::mem;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::LazyLock;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::VirtAddr;

const INTERRUPT_CONTROLLER_1_OFFSET: u8 = 32;
const SPURIOUS_INTERRUPT_VECTOR: u8 = u8::MAX;
const UNEXPECTED_EXTERNAL_INTERRUPT_VECTOR_START: u8 = INTERRUPT_CONTROLLER_1_OFFSET + 2;
const RAW_TIMER_INTERRUPT_FRAME_INSTRUCTION_POINTER_OFFSET: usize = 0;
const RAW_TIMER_INTERRUPT_FRAME_CODE_SEGMENT_OFFSET: usize = 8;
const RAW_TIMER_INTERRUPT_FRAME_CPU_FLAGS_OFFSET: usize = 16;
const RAW_TIMER_INTERRUPT_FRAME_STACK_POINTER_OFFSET: usize = 24;
const RAW_TIMER_INTERRUPT_FRAME_STACK_SEGMENT_OFFSET: usize = 32;
const RAW_TIMER_INTERRUPT_FRAME_RAX_OFFSET: usize = 40;
const RAW_TIMER_INTERRUPT_FRAME_RBX_OFFSET: usize = 48;
const RAW_TIMER_INTERRUPT_FRAME_RCX_OFFSET: usize = 56;
const RAW_TIMER_INTERRUPT_FRAME_RDX_OFFSET: usize = 64;
const RAW_TIMER_INTERRUPT_FRAME_RSI_OFFSET: usize = 72;
const RAW_TIMER_INTERRUPT_FRAME_RDI_OFFSET: usize = 80;
const RAW_TIMER_INTERRUPT_FRAME_RBP_OFFSET: usize = 88;
const RAW_TIMER_INTERRUPT_FRAME_R8_OFFSET: usize = 96;
const RAW_TIMER_INTERRUPT_FRAME_R9_OFFSET: usize = 104;
const RAW_TIMER_INTERRUPT_FRAME_R10_OFFSET: usize = 112;
const RAW_TIMER_INTERRUPT_FRAME_R11_OFFSET: usize = 120;
const RAW_TIMER_INTERRUPT_FRAME_R12_OFFSET: usize = 128;
const RAW_TIMER_INTERRUPT_FRAME_R13_OFFSET: usize = 136;
const RAW_TIMER_INTERRUPT_FRAME_R14_OFFSET: usize = 144;
const RAW_TIMER_INTERRUPT_FRAME_R15_OFFSET: usize = 152;
const RAW_TIMER_INTERRUPT_FRAME_BYTES: usize = 160;

static TICKS: AtomicU64 = AtomicU64::new(0);
static TIMER_TICK_PROCESSOR: AtomicUsize = AtomicUsize::new(0);
static KEYBOARD_BYTE_PROCESSOR: AtomicUsize = AtomicUsize::new(0);
static MOUSE_BYTE_PROCESSOR: AtomicUsize = AtomicUsize::new(0);
static PAGE_FAULT_REPORTER: AtomicUsize = AtomicUsize::new(0);
static SPURIOUS_INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);
static UNEXPECTED_EXTERNAL_INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Kernel callback invoked after each timer interrupt is acknowledged.
pub type TimerTickProcessor = fn(&TimerInterruptFrame);

/// Kernel callbacks invoked by architecture interrupt handlers.
#[derive(Clone, Copy)]
pub struct InterruptProcessors {
    /// Called after each timer interrupt is acknowledged.
    pub timer_tick: TimerTickProcessor,
    /// Called with each keyboard byte received from hardware.
    pub keyboard_byte: fn(u8),
    /// Called with each mouse byte received from hardware.
    pub mouse_byte: fn(u8),
}

/// Callback invoked before the page fault handler panics.
pub type PageFaultReporter = fn(PageFaultReport);

/// IDT vector diagnostics for unexpected external interrupts.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct InterruptVectorDiagnostics {
    spurious_interrupt_vector: u8,
    spurious_interrupt_count: u64,
    unexpected_external_interrupt_count: u64,
}

impl InterruptVectorDiagnostics {
    const fn new(
        spurious_interrupt_vector: u8,
        spurious_interrupt_count: u64,
        unexpected_external_interrupt_count: u64,
    ) -> Self {
        Self {
            spurious_interrupt_vector,
            spurious_interrupt_count,
            unexpected_external_interrupt_count,
        }
    }

    /// Return the IDT vector reserved for Local APIC spurious interrupts.
    pub const fn spurious_interrupt_vector(self) -> u8 {
        self.spurious_interrupt_vector
    }

    /// Return the number of Local APIC spurious interrupts observed.
    pub const fn spurious_interrupt_count(self) -> u64 {
        self.spurious_interrupt_count
    }

    /// Return the number of unexpected external interrupts observed.
    pub const fn unexpected_external_interrupt_count(self) -> u64 {
        self.unexpected_external_interrupt_count
    }
}

/// Return the number of timer ticks since interrupt initialization.
pub fn get_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Return IDT vector diagnostics for interrupt-controller smoke checks.
pub fn get_interrupt_vector_diagnostics() -> InterruptVectorDiagnostics {
    InterruptVectorDiagnostics::new(
        SPURIOUS_INTERRUPT_VECTOR,
        SPURIOUS_INTERRUPT_COUNT.load(Ordering::Acquire),
        UNEXPECTED_EXTERNAL_INTERRUPT_COUNT.load(Ordering::Acquire),
    )
}

/// Register kernel callbacks invoked by architecture interrupt handlers.
pub fn register_processors(processors: InterruptProcessors) {
    TIMER_TICK_PROCESSOR.store(processors.timer_tick as usize, Ordering::Release);
    KEYBOARD_BYTE_PROCESSOR.store(processors.keyboard_byte as usize, Ordering::Release);
    MOUSE_BYTE_PROCESSOR.store(processors.mouse_byte as usize, Ordering::Release);
}

/// Register the kernel page-fault diagnostic reporter.
pub fn register_page_fault_reporter(reporter: PageFaultReporter) {
    PAGE_FAULT_REPORTER.store(reporter as usize, Ordering::Release);
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

#[repr(C)]
struct RawTimerInterruptFrame {
    instruction_pointer: u64,
    code_segment: u64,
    cpu_flags: u64,
    stack_pointer: u64,
    stack_segment: u64,
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
}

const _: () = {
    assert!(mem::size_of::<RawTimerInterruptFrame>() == RAW_TIMER_INTERRUPT_FRAME_BYTES);
    assert!(
        mem::offset_of!(RawTimerInterruptFrame, instruction_pointer)
            == RAW_TIMER_INTERRUPT_FRAME_INSTRUCTION_POINTER_OFFSET
    );
    assert!(
        mem::offset_of!(RawTimerInterruptFrame, code_segment)
            == RAW_TIMER_INTERRUPT_FRAME_CODE_SEGMENT_OFFSET
    );
    assert!(
        mem::offset_of!(RawTimerInterruptFrame, cpu_flags)
            == RAW_TIMER_INTERRUPT_FRAME_CPU_FLAGS_OFFSET
    );
    assert!(
        mem::offset_of!(RawTimerInterruptFrame, stack_pointer)
            == RAW_TIMER_INTERRUPT_FRAME_STACK_POINTER_OFFSET
    );
    assert!(
        mem::offset_of!(RawTimerInterruptFrame, stack_segment)
            == RAW_TIMER_INTERRUPT_FRAME_STACK_SEGMENT_OFFSET
    );
    assert!(mem::offset_of!(RawTimerInterruptFrame, rax) == RAW_TIMER_INTERRUPT_FRAME_RAX_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, rbx) == RAW_TIMER_INTERRUPT_FRAME_RBX_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, rcx) == RAW_TIMER_INTERRUPT_FRAME_RCX_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, rdx) == RAW_TIMER_INTERRUPT_FRAME_RDX_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, rsi) == RAW_TIMER_INTERRUPT_FRAME_RSI_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, rdi) == RAW_TIMER_INTERRUPT_FRAME_RDI_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, rbp) == RAW_TIMER_INTERRUPT_FRAME_RBP_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, r8) == RAW_TIMER_INTERRUPT_FRAME_R8_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, r9) == RAW_TIMER_INTERRUPT_FRAME_R9_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, r10) == RAW_TIMER_INTERRUPT_FRAME_R10_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, r11) == RAW_TIMER_INTERRUPT_FRAME_R11_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, r12) == RAW_TIMER_INTERRUPT_FRAME_R12_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, r13) == RAW_TIMER_INTERRUPT_FRAME_R13_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, r14) == RAW_TIMER_INTERRUPT_FRAME_R14_OFFSET);
    assert!(mem::offset_of!(RawTimerInterruptFrame, r15) == RAW_TIMER_INTERRUPT_FRAME_R15_OFFSET);
};

extern "C" {
    fn timer_interrupt_handler_entry();
}

#[derive(Clone, Copy)]
struct InterruptEntryAddress(VirtAddr);

impl InterruptEntryAddress {
    fn from_function(function: unsafe extern "C" fn()) -> Self {
        let pointer = function as *const ();
        let address =
            u64::try_from(pointer.addr()).expect("interrupt entry address must fit in u64");
        Self(VirtAddr::new(address))
    }

    const fn as_x86_address(self) -> VirtAddr {
        self.0
    }

    fn as_u64(self) -> u64 {
        self.0.as_u64()
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

    // SAFETY: `timer_interrupt_handler_entry` is an interrupt entry stub that
    // preserves registers, calls the Rust timer hook, and returns with `iretq`.
    unsafe {
        let timer_entry_address =
            InterruptEntryAddress::from_function(timer_interrupt_handler_entry);
        table[InterruptIndex::Timer.as_u8()].set_handler_addr(timer_entry_address.as_x86_address());
    }
    table[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);
    table[InterruptIndex::Mouse.as_u8()].set_handler_fn(mouse_interrupt_handler);
    install_interrupt_vector_diagnostics(&mut table);
    table
});

pub fn initialize() {
    INTERRUPT_DESCRIPTOR_TABLE.load();
    let timer_entry_address = InterruptEntryAddress::from_function(timer_interrupt_handler_entry);
    crate::log_info!(
        "arch",
        "IDT timer entry initialized: address={:#x} timer_entry_typed=true",
        timer_entry_address.as_u64()
    );
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    let fault_address = Cr2::read_raw();
    let report = PageFaultReport::new(
        PageFaultAddress::new(fault_address),
        PageFaultErrorBits::new(error_code.bits()),
        PageFaultInstructionPointer::new(stack_frame.instruction_pointer.as_u64()),
    );
    call_page_fault_reporter(report);
    panic!("[EXCEPT] PAGE FAULT\nAddr: {fault_address:#x}\nCode: {error_code:?}\n{stack_frame:#?}");
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!("[EXCEPT] GPF\nCode: {error_code:#x}\n{stack_frame:#?}");
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("[EXCEPT] BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("[EXCEPT] DOUBLE FAULT\n{stack_frame:#?}");
}

/// Push a raw timer interrupt frame from the assembly interrupt entry stub.
///
/// # Panics
///
/// Panics if `frame` is null or cannot be represented as a `u64` address.
///
/// # Safety
///
/// `frame` must point to a stack-resident `RawTimerInterruptFrame` populated by
/// `timer_interrupt_handler_entry`.
#[no_mangle]
pub unsafe extern "C" fn push_timer_interrupt_frame(frame: *const u64) {
    assert!(
        !frame.is_null(),
        "timer interrupt frame pointer must be non-null"
    );
    let frame_storage_address =
        u64::try_from(frame.addr()).expect("timer interrupt frame pointer must fit in u64");
    let raw_frame = frame.cast::<RawTimerInterruptFrame>();
    // SAFETY: The assembly timer entry passes a non-null pointer to the raw
    // frame it populated on the current kernel stack.
    let raw_frame = unsafe { &*raw_frame };
    TICKS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: Notify EOI to the configured interrupt controller backend to
    // allow future interrupts.
    unsafe {
        crate::arch::x86_64::interrupt_controller::notify_end_of_interrupt(
            InterruptIndex::Timer.as_u8(),
        );
    }
    let frame = TimerInterruptFrame {
        frame_storage_address,
        instruction_pointer: raw_frame.instruction_pointer,
        code_segment: raw_frame.code_segment,
        cpu_flags: raw_frame.cpu_flags,
        stack_pointer: raw_frame.stack_pointer,
        stack_segment: raw_frame.stack_segment,
        rax: raw_frame.rax,
        rbx: raw_frame.rbx,
        rcx: raw_frame.rcx,
        rdx: raw_frame.rdx,
        rsi: raw_frame.rsi,
        rdi: raw_frame.rdi,
        rbp: raw_frame.rbp,
        r8: raw_frame.r8,
        r9: raw_frame.r9,
        r10: raw_frame.r10,
        r11: raw_frame.r11,
        r12: raw_frame.r12,
        r13: raw_frame.r13,
        r14: raw_frame.r14,
        r15: raw_frame.r15,
    };
    call_timer_tick_processor(&frame);
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let status = status_read();
    if (status & 0x20) == 0 {
        // SAFETY: Port 0x60 is the PS/2 data port and status bit indicates keyboard data.
        let scancode = unsafe { Port::<u8>::new(0x60).read() };
        call_keyboard_byte_processor(scancode);
    }
    // SAFETY: Notify EOI to the configured interrupt controller backend to
    // allow future interrupts.
    unsafe {
        crate::arch::x86_64::interrupt_controller::notify_end_of_interrupt(
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
    // SAFETY: Notify EOI to the configured interrupt controller backend to
    // allow future interrupts.
    unsafe {
        crate::arch::x86_64::interrupt_controller::notify_end_of_interrupt(
            InterruptIndex::Mouse.as_u8(),
        );
    }
}

extern "x86-interrupt" fn push_spurious_interrupt(_stack_frame: InterruptStackFrame) {
    SPURIOUS_INTERRUPT_COUNT.fetch_add(1, Ordering::Relaxed);
}

extern "x86-interrupt" fn push_unexpected_external_interrupt(_stack_frame: InterruptStackFrame) {
    UNEXPECTED_EXTERNAL_INTERRUPT_COUNT.fetch_add(1, Ordering::Relaxed);
    // SAFETY: This handler is installed only for external interrupt vectors.
    // When APIC routing is active, the Local APIC requires one EOI for an
    // accepted interrupt before more interrupts of the same priority can arrive.
    unsafe {
        crate::arch::x86_64::interrupt_controller::notify_unexpected_external_end_of_interrupt();
    }
}

fn install_interrupt_vector_diagnostics(table: &mut InterruptDescriptorTable) {
    let mut vector = UNEXPECTED_EXTERNAL_INTERRUPT_VECTOR_START;
    while vector < SPURIOUS_INTERRUPT_VECTOR {
        if !is_known_external_interrupt_vector(vector) {
            table[vector].set_handler_fn(push_unexpected_external_interrupt);
        }
        vector += 1;
    }
    table[SPURIOUS_INTERRUPT_VECTOR].set_handler_fn(push_spurious_interrupt);
}

fn is_known_external_interrupt_vector(vector: u8) -> bool {
    vector == InterruptIndex::Timer.as_u8()
        || vector == InterruptIndex::Keyboard.as_u8()
        || vector == InterruptIndex::Mouse.as_u8()
}

fn status_read() -> u8 {
    // SAFETY: Port 0x64 is the PS/2 controller status port.
    unsafe { Port::<u8>::new(0x64).read() }
}

fn call_timer_tick_processor(frame: &TimerInterruptFrame) {
    let processor = TIMER_TICK_PROCESSOR.load(Ordering::Acquire);
    if processor == 0 {
        return;
    }

    // SAFETY: register_processors stores only valid timer tick processor pointers.
    let processor: TimerTickProcessor = unsafe { core::mem::transmute(processor) };
    processor(frame);
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

fn call_page_fault_reporter(report: PageFaultReport) {
    let reporter = PAGE_FAULT_REPORTER.load(Ordering::Acquire);
    if reporter == 0 {
        return;
    }

    // SAFETY: register_page_fault_reporter stores only valid PageFaultReporter
    // function pointers.
    let reporter: PageFaultReporter = unsafe { core::mem::transmute(reporter) };
    reporter(report);
}
