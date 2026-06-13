//! # `kernel::interrupt`
//!
//! ## Owns
//! - Kernel-side interrupt event routing
//! - Page-fault diagnostic formatting
//!
//! ## Does NOT own
//! - Architecture-specific interrupt descriptor tables (-> `arch`)
//! - Hardware interrupt acknowledgement (-> `arch`)
//! - Input byte queues (-> `kernel::driver::input`)
//!
//! ## Public API
//! - [`process_timer_tick`] - Route timer ticks to the scheduler
//! - [`push_keyboard_byte`] - Route keyboard bytes to the keyboard queue
//! - [`push_mouse_byte`] - Route mouse bytes to the mouse queue
//! - [`process_page_fault`] - Log page-fault diagnostics
//! - [`set_syscall_kernel_stack_top`] - Install the next SYSCALL kernel stack
//! - [`syscall_entry`] - Ring 3 syscall entry

use crate::kernel::task::context::UserTrapFrame;
use crate::shared::TimerInterruptFrame;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static SYSCALL_KERNEL_STACK_TOP: AtomicU64 = AtomicU64::new(0);
static SYSCALL_ENTRY_USER_STACK_POINTER: AtomicU64 = AtomicU64::new(0);
static SYSCALL_ENTRY_SYSCALL_NUMBER: AtomicU64 = AtomicU64::new(0);
static USER_TIMER_FRAME_REPORTED: AtomicBool = AtomicBool::new(false);

/// Route one timer interrupt tick to the kernel scheduler.
pub fn process_timer_tick(frame: &TimerInterruptFrame) {
    let interrupted_user_mode = frame.is_user_mode();
    if interrupted_user_mode {
        crate::kernel::task::record_current_user_interrupt_trap_frame(
            timer_frame_to_user_trap_frame(frame),
            frame.frame_storage_address,
        );
        report_user_timer_frame_once(frame);
    }
    crate::kernel::task::process_timer_tick(interrupted_user_mode);
}

/// Route one keyboard byte to the keyboard input queue.
pub fn push_keyboard_byte(byte: u8) {
    crate::kernel::driver::input::keyboard::push_scancode(byte);
}

/// Route one mouse byte to the mouse input queue.
pub fn push_mouse_byte(byte: u8) {
    crate::kernel::driver::input::mouse::push_byte(byte);
}

/// Install the guarded kernel stack top used by future `SYSCALL` entries.
///
/// # Panics
///
/// Panics if `stack_top` is zero.
pub fn set_syscall_kernel_stack_top(stack_top: u64) {
    assert!(stack_top != 0, "syscall kernel stack top must be non-zero");
    SYSCALL_KERNEL_STACK_TOP.store(stack_top, Ordering::Release);
}

/// Log page-fault diagnostics before the architecture handler panics.
pub fn process_page_fault(fault_address: u64, error_code: u64, instruction_pointer: u64) {
    let task_id = crate::kernel::task::get_current_task_id();
    if let Some(guard_fault) = crate::kernel::task::get_kernel_stack_guard_fault(fault_address) {
        crate::log_error!(
            "fault",
            "Kernel stack guard page fault: owner={} task={} fault_address={:#018x} guard={:#018x} writable_start={:#018x} stack_top={:#018x} access={} mode={} present={} instruction={:#018x} raw_error={:#x}",
            guard_fault.owner().as_str(),
            guard_fault.task_identifier(),
            fault_address,
            guard_fault.guard_page_start(),
            guard_fault.writable_start(),
            guard_fault.stack_top(),
            PageFaultAccess::from_error_code(error_code).as_str(),
            PageFaultMode::from_error_code(error_code).as_str(),
            PageFaultPresence::from_error_code(error_code).as_str(),
            instruction_pointer,
            error_code
        );
    }
    crate::log_error!(
        "fault",
        "Page fault: task={} address={:#018x} access={} mode={} present={} instruction={:#018x} raw_error={:#x}",
        TaskIdentifierDisplay(task_id),
        fault_address,
        PageFaultAccess::from_error_code(error_code).as_str(),
        PageFaultMode::from_error_code(error_code).as_str(),
        PageFaultPresence::from_error_code(error_code).as_str(),
        instruction_pointer,
        error_code
    );
}

struct TaskIdentifierDisplay(Option<u64>);

impl core::fmt::Display for TaskIdentifierDisplay {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(task_id) => write!(formatter, "{task_id}"),
            None => formatter.write_str("unknown"),
        }
    }
}

fn report_user_timer_frame_once(frame: &TimerInterruptFrame) {
    if USER_TIMER_FRAME_REPORTED.swap(true, Ordering::AcqRel) {
        return;
    }

    crate::log_info!(
        "task",
        "User timer interrupt frame observed: task={} rip={:#x} rsp={:#x} cs={:#x} ss={:#x} rflags={:#x}",
        TaskIdentifierDisplay(crate::kernel::task::get_current_task_id()),
        frame.instruction_pointer,
        frame.stack_pointer,
        frame.code_segment,
        frame.stack_segment,
        frame.cpu_flags
    );
}

fn timer_frame_to_user_trap_frame(frame: &TimerInterruptFrame) -> UserTrapFrame {
    UserTrapFrame {
        instruction_pointer: frame.instruction_pointer,
        code_segment: frame.code_segment,
        cpu_flags: frame.cpu_flags,
        stack_pointer: frame.stack_pointer,
        stack_segment: frame.stack_segment,
        rax: frame.rax,
        rbx: frame.rbx,
        rcx: frame.rcx,
        rdx: frame.rdx,
        rsi: frame.rsi,
        rdi: frame.rdi,
        rbp: frame.rbp,
        r8: frame.r8,
        r9: frame.r9,
        r10: frame.r10,
        r11: frame.r11,
        r12: frame.r12,
        r13: frame.r13,
        r14: frame.r14,
        r15: frame.r15,
    }
}

#[derive(Clone, Copy)]
enum PageFaultAccess {
    Read,
    Write,
    InstructionFetch,
}

impl PageFaultAccess {
    const WRITE_BIT: u64 = 1 << 1;
    const INSTRUCTION_FETCH_BIT: u64 = 1 << 4;

    fn from_error_code(error_code: u64) -> Self {
        if error_code & Self::INSTRUCTION_FETCH_BIT != 0 {
            Self::InstructionFetch
        } else if error_code & Self::WRITE_BIT != 0 {
            Self::Write
        } else {
            Self::Read
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::InstructionFetch => "instruction_fetch",
        }
    }
}

#[derive(Clone, Copy)]
enum PageFaultMode {
    Kernel,
    User,
}

impl PageFaultMode {
    const USER_BIT: u64 = 1 << 2;

    fn from_error_code(error_code: u64) -> Self {
        if error_code & Self::USER_BIT != 0 {
            Self::User
        } else {
            Self::Kernel
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Kernel => "kernel",
            Self::User => "user",
        }
    }
}

#[derive(Clone, Copy)]
enum PageFaultPresence {
    NotPresent,
    ProtectionViolation,
}

impl PageFaultPresence {
    const PRESENT_BIT: u64 = 1;

    fn from_error_code(error_code: u64) -> Self {
        if error_code & Self::PRESENT_BIT != 0 {
            Self::ProtectionViolation
        } else {
            Self::NotPresent
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::NotPresent => "not_present",
            Self::ProtectionViolation => "protection_violation",
        }
    }
}

/// Kernel entry point for the `SYSCALL` instruction from Ring 3.
///
/// # Safety
///
/// Called directly by the CPU on `SYSCALL`; register state is raw.
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        "mov qword ptr [rip + {entry_user_stack_pointer}], rsp",
        "mov qword ptr [rip + {entry_syscall_number}], rax",
        "mov rsp, qword ptr [rip + {syscall_kernel_stack_top}]",
        "sub rsp, 192",
        "mov qword ptr [rsp + 32], rcx",
        "mov qword ptr [rsp + 40], 0",
        "mov qword ptr [rsp + 48], r11",
        "mov rax, qword ptr [rip + {entry_user_stack_pointer}]",
        "mov qword ptr [rsp + 56], rax",
        "mov qword ptr [rsp + 64], 0",
        "mov rax, qword ptr [rip + {entry_syscall_number}]",
        "mov qword ptr [rsp + 72], rax",
        "mov qword ptr [rsp + 80], rbx",
        "mov qword ptr [rsp + 88], rcx",
        "mov qword ptr [rsp + 96], rdx",
        "mov qword ptr [rsp + 104], rsi",
        "mov qword ptr [rsp + 112], rdi",
        "mov qword ptr [rsp + 120], rbp",
        "mov qword ptr [rsp + 128], r8",
        "mov qword ptr [rsp + 136], r9",
        "mov qword ptr [rsp + 144], r10",
        "mov qword ptr [rsp + 152], r11",
        "mov qword ptr [rsp + 160], r12",
        "mov qword ptr [rsp + 168], r13",
        "mov qword ptr [rsp + 176], r14",
        "mov qword ptr [rsp + 184], r15",
        "lea rcx, [rsp + 32]",
        "call {dispatcher}",
        "cmp rax, {exit_sentinel}",
        "je 2f",
        "cmp rax, {block_sentinel}",
        "je 2f",
        "mov rbx, qword ptr [rsp + 80]",
        "mov rdx, qword ptr [rsp + 96]",
        "mov rsi, qword ptr [rsp + 104]",
        "mov rdi, qword ptr [rsp + 112]",
        "mov rbp, qword ptr [rsp + 120]",
        "mov r8, qword ptr [rsp + 128]",
        "mov r9, qword ptr [rsp + 136]",
        "mov r10, qword ptr [rsp + 144]",
        "mov r12, qword ptr [rsp + 160]",
        "mov r13, qword ptr [rsp + 168]",
        "mov r14, qword ptr [rsp + 176]",
        "mov r15, qword ptr [rsp + 184]",
        "mov rcx, qword ptr [rsp + 32]",
        "mov r11, qword ptr [rsp + 48]",
        "mov rax, qword ptr [rsp + 72]",
        "mov rsp, qword ptr [rsp + 56]",
        "sysretq",
        "2:",
        "call {get_return_stack}",
        "mov rsp, rax",
        "ret",
        dispatcher = sym crate::kernel::syscall::syscall_dispatch_from_trap_frame,
        get_return_stack = sym crate::kernel::task::process_lifecycle::get_user_return_stack,
        syscall_kernel_stack_top = sym SYSCALL_KERNEL_STACK_TOP,
        entry_user_stack_pointer = sym SYSCALL_ENTRY_USER_STACK_POINTER,
        entry_syscall_number = sym SYSCALL_ENTRY_SYSCALL_NUMBER,
        exit_sentinel = const crate::kernel::syscall::USER_EXIT_SENTINEL,
        block_sentinel = const crate::kernel::syscall::USER_BLOCK_SENTINEL,
    );
}
