//! Thin wrappers for the Linux-like ManaOS syscall ABI.

use core::arch::asm;

/// Linux-compatible read syscall number.
pub const SYS_READ: usize = 0;
/// Linux-compatible write syscall number.
pub const SYS_WRITE: usize = 1;
/// Linux-compatible open syscall number.
pub const SYS_OPEN: usize = 2;
/// Linux-compatible close syscall number.
pub const SYS_CLOSE: usize = 3;
/// Linux-compatible exit syscall number.
pub const SYS_EXIT: usize = 60;

/// Bad address error return value as a signed syscall result.
pub const ERROR_BAD_ADDRESS: isize = -14;

/// Invoke a ManaOS syscall with one argument.
#[inline(always)]
pub fn syscall1(syscall_number: usize, first_argument: usize) -> isize {
    let result: usize;

    // SAFETY: The register assignments match the ManaOS syscall ABI:
    // rax=syscall number and rdi=first argument. `syscall` clobbers rcx/r11.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") syscall_number => result,
            inlateout("rdi") first_argument => _,
            lateout("rcx") _,
            lateout("rdx") _,
            lateout("rsi") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
            lateout("r11") _,
            options(nostack)
        );
    }

    result as isize
}

/// Invoke a ManaOS syscall with two arguments.
#[inline(always)]
pub fn syscall2(syscall_number: usize, first_argument: usize, second_argument: usize) -> isize {
    let result: usize;

    // SAFETY: The register assignments match the ManaOS syscall ABI:
    // rax=syscall number, rdi=first argument, and rsi=second argument.
    // `syscall` clobbers rcx/r11.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") syscall_number => result,
            inlateout("rdi") first_argument => _,
            inlateout("rsi") second_argument => _,
            lateout("rcx") _,
            lateout("rdx") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
            lateout("r11") _,
            options(nostack)
        );
    }

    result as isize
}

/// Invoke a ManaOS syscall with three arguments.
#[inline(always)]
pub fn syscall3(
    syscall_number: usize,
    first_argument: usize,
    second_argument: usize,
    third_argument: usize,
) -> isize {
    let result: usize;

    // SAFETY: The register assignments match the ManaOS syscall ABI:
    // rax=syscall number, rdi/rsi/rdx=arguments. `syscall` clobbers rcx/r11.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") syscall_number => result,
            inlateout("rdi") first_argument => _,
            inlateout("rsi") second_argument => _,
            inlateout("rdx") third_argument => _,
            lateout("rcx") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
            lateout("r11") _,
            options(nostack)
        );
    }

    result as isize
}

/// Write `buffer` to an open file descriptor.
#[inline(always)]
pub fn write(file_descriptor: usize, buffer: &[u8]) -> isize {
    syscall3(
        SYS_WRITE,
        file_descriptor,
        buffer.as_ptr() as usize,
        buffer.len(),
    )
}

/// Read bytes from an open file descriptor into `buffer`.
#[inline(always)]
pub fn read(file_descriptor: usize, buffer: &mut [u8]) -> isize {
    syscall3(
        SYS_READ,
        file_descriptor,
        buffer.as_mut_ptr() as usize,
        buffer.len(),
    )
}

/// Open a null-terminated path.
#[inline(always)]
pub fn open(path: &[u8]) -> isize {
    syscall1(SYS_OPEN, path.as_ptr() as usize)
}

/// Close an open file descriptor.
#[inline(always)]
pub fn close(file_descriptor: usize) -> isize {
    syscall1(SYS_CLOSE, file_descriptor)
}

/// Terminate the current user task.
#[inline(always)]
pub fn exit(code: usize) -> ! {
    let _ = syscall1(SYS_EXIT, code);

    loop {
        core::hint::spin_loop();
    }
}
