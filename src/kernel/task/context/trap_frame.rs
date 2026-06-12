//! User trap frame layout.

use core::mem;

const USER_TRAP_FRAME_INSTRUCTION_POINTER_OFFSET: usize = 0;
const USER_TRAP_FRAME_CODE_SEGMENT_OFFSET: usize = 8;
const USER_TRAP_FRAME_CPU_FLAGS_OFFSET: usize = 16;
const USER_TRAP_FRAME_STACK_POINTER_OFFSET: usize = 24;
const USER_TRAP_FRAME_STACK_SEGMENT_OFFSET: usize = 32;
const USER_TRAP_FRAME_RAX_OFFSET: usize = 40;
const USER_TRAP_FRAME_RBX_OFFSET: usize = 48;
const USER_TRAP_FRAME_RCX_OFFSET: usize = 56;
const USER_TRAP_FRAME_RDX_OFFSET: usize = 64;
const USER_TRAP_FRAME_RSI_OFFSET: usize = 72;
const USER_TRAP_FRAME_RDI_OFFSET: usize = 80;
const USER_TRAP_FRAME_RBP_OFFSET: usize = 88;
const USER_TRAP_FRAME_R8_OFFSET: usize = 96;
const USER_TRAP_FRAME_R9_OFFSET: usize = 104;
const USER_TRAP_FRAME_R10_OFFSET: usize = 112;
const USER_TRAP_FRAME_R11_OFFSET: usize = 120;
const USER_TRAP_FRAME_R12_OFFSET: usize = 128;
const USER_TRAP_FRAME_R13_OFFSET: usize = 136;
const USER_TRAP_FRAME_R14_OFFSET: usize = 144;
const USER_TRAP_FRAME_R15_OFFSET: usize = 152;
const USER_TRAP_FRAME_BYTES: usize = 160;

/// Full user-mode register frame required to resume a preempted user task.
///
/// This is a design contract for the future interrupt and syscall paths. The
/// current boot path still enters user code through [`super::UserTaskContext`]
/// and does not save or restore this frame yet.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct UserTrapFrame {
    /// User instruction pointer restored by `iretq`.
    pub instruction_pointer: u64,
    /// Ring 3 code segment selector restored by `iretq`.
    pub code_segment: u64,
    /// User CPU flags restored by `iretq`.
    pub cpu_flags: u64,
    /// User stack pointer restored by `iretq`.
    pub stack_pointer: u64,
    /// Ring 3 stack segment selector restored by `iretq`.
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

const _: () = {
    assert!(mem::size_of::<UserTrapFrame>() == USER_TRAP_FRAME_BYTES);
    assert!(
        mem::offset_of!(UserTrapFrame, instruction_pointer)
            == USER_TRAP_FRAME_INSTRUCTION_POINTER_OFFSET
    );
    assert!(mem::offset_of!(UserTrapFrame, code_segment) == USER_TRAP_FRAME_CODE_SEGMENT_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, cpu_flags) == USER_TRAP_FRAME_CPU_FLAGS_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, stack_pointer) == USER_TRAP_FRAME_STACK_POINTER_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, stack_segment) == USER_TRAP_FRAME_STACK_SEGMENT_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, rax) == USER_TRAP_FRAME_RAX_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, rbx) == USER_TRAP_FRAME_RBX_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, rcx) == USER_TRAP_FRAME_RCX_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, rdx) == USER_TRAP_FRAME_RDX_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, rsi) == USER_TRAP_FRAME_RSI_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, rdi) == USER_TRAP_FRAME_RDI_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, rbp) == USER_TRAP_FRAME_RBP_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, r8) == USER_TRAP_FRAME_R8_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, r9) == USER_TRAP_FRAME_R9_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, r10) == USER_TRAP_FRAME_R10_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, r11) == USER_TRAP_FRAME_R11_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, r12) == USER_TRAP_FRAME_R12_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, r13) == USER_TRAP_FRAME_R13_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, r14) == USER_TRAP_FRAME_R14_OFFSET);
    assert!(mem::offset_of!(UserTrapFrame, r15) == USER_TRAP_FRAME_R15_OFFSET);
};
