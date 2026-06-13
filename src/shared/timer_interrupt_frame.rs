//! Timer interrupt register frame shared across architecture and kernel code.

const USER_PRIVILEGE_LEVEL_BITS: u64 = 0b11;

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
    /// Return `true` when the interrupted code segment belongs to Ring 3.
    pub fn is_user_mode(&self) -> bool {
        self.code_segment & USER_PRIVILEGE_LEVEL_BITS == USER_PRIVILEGE_LEVEL_BITS
    }
}
