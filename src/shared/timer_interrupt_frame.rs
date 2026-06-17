//! Timer interrupt register frame shared across architecture and kernel code.

const USER_PRIVILEGE_LEVEL_BITS: u64 = 0b11;

/// Stack address where the architecture timer entry stored the raw frame.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TimerFrameStorageAddress(u64);

impl TimerFrameStorageAddress {
    /// Create a storage-address wrapper from the architecture timer entry.
    pub const fn new(address: u64) -> Self {
        Self(address)
    }

    /// Return the raw storage address for final diagnostics or conversion.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Instruction pointer captured by the architecture timer entry.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TimerFrameInstructionPointer(u64);

impl TimerFrameInstructionPointer {
    /// Create an instruction-pointer wrapper from the architecture timer frame.
    pub const fn new(address: u64) -> Self {
        Self(address)
    }

    /// Return the raw instruction pointer for final diagnostics or conversion.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Stack pointer captured by the architecture timer entry.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TimerFrameStackPointer(u64);

impl TimerFrameStackPointer {
    /// Create a stack-pointer wrapper from the architecture timer frame.
    pub const fn new(address: u64) -> Self {
        Self(address)
    }

    /// Return the raw stack pointer for final diagnostics or conversion.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Complete register snapshot captured by the architecture timer interrupt path.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TimerInterruptFrame {
    /// Address of the architecture-owned stack storage used for the raw frame.
    pub frame_storage_address: u64,
    /// Interrupted instruction pointer.
    pub instruction_pointer: u64,
    /// Interrupted code segment selector.
    pub code_segment: u64,
    /// Interrupted CPU flags.
    pub cpu_flags: u64,
    /// Interrupted stack pointer when the frame came from user mode.
    pub stack_pointer: u64,
    /// Interrupted stack segment selector when the frame came from user mode.
    pub stack_segment: u64,
    /// General-purpose `rax` register.
    pub rax: u64,
    /// General-purpose `rbx` register.
    pub rbx: u64,
    /// General-purpose `rcx` register.
    pub rcx: u64,
    /// General-purpose `rdx` register.
    pub rdx: u64,
    /// General-purpose `rsi` register.
    pub rsi: u64,
    /// General-purpose `rdi` register.
    pub rdi: u64,
    /// General-purpose `rbp` register.
    pub rbp: u64,
    /// General-purpose `r8` register.
    pub r8: u64,
    /// General-purpose `r9` register.
    pub r9: u64,
    /// General-purpose `r10` register.
    pub r10: u64,
    /// General-purpose `r11` register.
    pub r11: u64,
    /// General-purpose `r12` register.
    pub r12: u64,
    /// General-purpose `r13` register.
    pub r13: u64,
    /// General-purpose `r14` register.
    pub r14: u64,
    /// General-purpose `r15` register.
    pub r15: u64,
}

impl TimerInterruptFrame {
    /// Return the stack storage address as a typed shared boundary value.
    pub const fn frame_storage_address(&self) -> TimerFrameStorageAddress {
        TimerFrameStorageAddress::new(self.frame_storage_address)
    }

    /// Return the interrupted instruction pointer as a typed shared boundary value.
    pub const fn instruction_pointer(&self) -> TimerFrameInstructionPointer {
        TimerFrameInstructionPointer::new(self.instruction_pointer)
    }

    /// Return the interrupted stack pointer as a typed shared boundary value.
    pub const fn stack_pointer(&self) -> TimerFrameStackPointer {
        TimerFrameStackPointer::new(self.stack_pointer)
    }

    /// Return `true` when the interrupted code segment belongs to Ring 3.
    pub fn is_user_mode(&self) -> bool {
        self.code_segment & USER_PRIVILEGE_LEVEL_BITS == USER_PRIVILEGE_LEVEL_BITS
    }
}

/// Verify timer interrupt frame address wrappers preserve their raw values.
pub fn verify_typed_timer_interrupt_frame() -> bool {
    let frame = TimerInterruptFrame {
        frame_storage_address: 0xffff_8000_0000_1000,
        instruction_pointer: 0x0000_4000_0000_0000,
        code_segment: USER_PRIVILEGE_LEVEL_BITS,
        cpu_flags: 0x202,
        stack_pointer: 0x0000_7fff_f000_0000,
        stack_segment: USER_PRIVILEGE_LEVEL_BITS,
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
    };

    frame.frame_storage_address().as_u64() == 0xffff_8000_0000_1000
        && frame.instruction_pointer().as_u64() == 0x0000_4000_0000_0000
        && frame.stack_pointer().as_u64() == 0x0000_7fff_f000_0000
        && frame.is_user_mode()
}
