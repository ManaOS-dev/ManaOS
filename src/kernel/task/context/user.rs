//! User first-entry context layout.

use core::mem;

use crate::kernel::memory::address::UserVirtualAddress;

const USER_CONTEXT_INSTRUCTION_POINTER_OFFSET: usize = 0;
const USER_CONTEXT_CODE_SEGMENT_OFFSET: usize = 8;
const USER_CONTEXT_CPU_FLAGS_OFFSET: usize = 16;
const USER_CONTEXT_STACK_POINTER_OFFSET: usize = 24;
const USER_CONTEXT_STACK_SEGMENT_OFFSET: usize = 32;
const USER_CONTEXT_ARGUMENT_COUNT_OFFSET: usize = 40;
const USER_CONTEXT_ARGUMENT_VALUES_POINTER_OFFSET: usize = 48;
const USER_CONTEXT_ENVIRONMENT_VALUES_POINTER_OFFSET: usize = 56;
const USER_CONTEXT_BYTES: usize = 64;

/// General-purpose user entry registers supplied at first instruction.
#[derive(Debug, Clone, Copy)]
pub struct UserEntryArguments {
    /// Number of entries in the `argv` pointer array.
    pub argument_count: u64,
    /// User virtual address of the null-terminated `argv` pointer array.
    pub argument_values_pointer: UserVirtualAddress,
    /// User virtual address of the null-terminated environment pointer array.
    pub environment_values_pointer: UserVirtualAddress,
}

/// User-mode transition frame and first-entry argument registers.
///
/// The field order is part of the `enter_user_mode*` assembly contract in
/// `arch/x86_64/context_switch.s`:
/// - offset 0: instruction pointer pushed into the `iretq` frame
/// - offset 8: code segment pushed into the `iretq` frame
/// - offset 16: CPU flags pushed into the `iretq` frame
/// - offset 24: stack pointer pushed into the `iretq` frame
/// - offset 32: stack segment pushed into the `iretq` frame
/// - offset 40: `argc` loaded into `rdi`
/// - offset 48: `argv` loaded into `rsi`
/// - offset 56: `envp` loaded into `rdx`
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct UserTaskContext {
    /// User entry point instruction pointer.
    pub instruction_pointer: u64,
    /// Ring 3 code segment selector.
    pub code_segment: u64,
    /// Initial CPU flags with interrupts enabled and IOPL set to zero.
    pub cpu_flags: u64,
    /// Top of the mapped user stack.
    pub stack_pointer: u64,
    /// Ring 3 stack segment selector.
    pub stack_segment: u64,
    /// Number of entries in the user `argv` pointer array.
    pub argument_count: u64,
    /// User virtual address of the null-terminated `argv` pointer array.
    pub argument_values_pointer: u64,
    /// User virtual address of the null-terminated environment pointer array.
    pub environment_values_pointer: u64,
}

impl UserTaskContext {
    /// Create an initial user-space context.
    ///
    /// # Safety
    ///
    /// `stack_top` must point one byte past a mapped, writable user-space stack
    /// page. `entry_point` must be a valid mapped user-space instruction
    /// address.
    pub unsafe fn new(
        entry_point: UserVirtualAddress,
        stack_top: UserVirtualAddress,
        entry_arguments: UserEntryArguments,
    ) -> Self {
        let selectors = crate::kernel::task::user_mode::get_selectors();
        assert!(
            selectors.code != 0 && selectors.data != 0,
            "user-mode selectors must be registered before spawning user tasks"
        );

        Self {
            instruction_pointer: entry_point.as_u64(),
            code_segment: u64::from(selectors.code),
            cpu_flags: 0x202,
            stack_pointer: stack_top.as_u64(),
            stack_segment: u64::from(selectors.data),
            argument_count: entry_arguments.argument_count,
            argument_values_pointer: entry_arguments.argument_values_pointer.as_u64(),
            environment_values_pointer: entry_arguments.environment_values_pointer.as_u64(),
        }
    }

    /// Return an immutable pointer suitable for the `enter_user_mode` stub.
    pub fn as_pointer(&self) -> *const u64 {
        core::ptr::addr_of!(self.instruction_pointer)
    }
}

const _: () = {
    assert!(mem::size_of::<UserTaskContext>() == USER_CONTEXT_BYTES);
    assert!(
        mem::offset_of!(UserTaskContext, instruction_pointer)
            == USER_CONTEXT_INSTRUCTION_POINTER_OFFSET
    );
    assert!(mem::offset_of!(UserTaskContext, code_segment) == USER_CONTEXT_CODE_SEGMENT_OFFSET);
    assert!(mem::offset_of!(UserTaskContext, cpu_flags) == USER_CONTEXT_CPU_FLAGS_OFFSET);
    assert!(mem::offset_of!(UserTaskContext, stack_pointer) == USER_CONTEXT_STACK_POINTER_OFFSET);
    assert!(mem::offset_of!(UserTaskContext, stack_segment) == USER_CONTEXT_STACK_SEGMENT_OFFSET);
    assert!(mem::offset_of!(UserTaskContext, argument_count) == USER_CONTEXT_ARGUMENT_COUNT_OFFSET);
    assert!(
        mem::offset_of!(UserTaskContext, argument_values_pointer)
            == USER_CONTEXT_ARGUMENT_VALUES_POINTER_OFFSET
    );
    assert!(
        mem::offset_of!(UserTaskContext, environment_values_pointer)
            == USER_CONTEXT_ENVIRONMENT_VALUES_POINTER_OFFSET
    );
};
