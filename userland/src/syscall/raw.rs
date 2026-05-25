//! Raw syscall instructions for the ManaOS user ABI.

use core::arch::asm;

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

/// Invoke a ManaOS syscall with four arguments.
#[inline(always)]
pub fn syscall4(
    syscall_number: usize,
    first_argument: usize,
    second_argument: usize,
    third_argument: usize,
    fourth_argument: usize,
) -> isize {
    let result: usize;

    // SAFETY: The register assignments match the ManaOS syscall ABI:
    // rax=syscall number, rdi/rsi/rdx/r10=arguments. `syscall` clobbers rcx/r11.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") syscall_number => result,
            inlateout("rdi") first_argument => _,
            inlateout("rsi") second_argument => _,
            inlateout("rdx") third_argument => _,
            inlateout("r10") fourth_argument => _,
            lateout("rcx") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r11") _,
            options(nostack)
        );
    }

    result as isize
}
