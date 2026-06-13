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
//! - [`syscall_entry`] - Ring 3 syscall entry

/// Route one timer interrupt tick to the kernel scheduler.
pub fn process_timer_tick() {
    crate::kernel::task::process_timer_tick();
}

/// Route one keyboard byte to the keyboard input queue.
pub fn push_keyboard_byte(byte: u8) {
    crate::kernel::driver::input::keyboard::push_scancode(byte);
}

/// Route one mouse byte to the mouse input queue.
pub fn push_mouse_byte(byte: u8) {
    crate::kernel::driver::input::mouse::push_byte(byte);
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
        "sub rsp, 192",
        "mov qword ptr [rsp + 32], rcx",
        "mov qword ptr [rsp + 40], 0",
        "mov qword ptr [rsp + 48], r11",
        "mov qword ptr [rsp + 72], rax",
        "lea rax, [rsp + 192]",
        "mov qword ptr [rsp + 56], rax",
        "mov qword ptr [rsp + 64], 0",
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
        "add rsp, 192",
        "sysretq",
        "2:",
        "call {get_return_stack}",
        "mov rsp, rax",
        "ret",
        dispatcher = sym crate::kernel::syscall::syscall_dispatch_from_trap_frame,
        get_return_stack = sym crate::kernel::task::process_lifecycle::get_user_exit_return_stack,
        exit_sentinel = const crate::kernel::syscall::USER_EXIT_SENTINEL,
    );
}
