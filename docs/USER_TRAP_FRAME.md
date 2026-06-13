# ManaOS User Trap Frame

This document defines the saved user register layout required before ManaOS can
preempt user tasks or resume user tasks after arbitrary interrupts and syscalls.

## Current Entry Context

`kernel::task::context::UserTaskContext` is an initial-entry frame. It is
converted into a `UserTrapFrame` before `arch/x86_64/context_switch.s`
restores user registers with:

- `rip`, `cs`, `rflags`, `rsp`, and `ss` for the `iretq` frame.
- `argc`, `argv`, and `envp` loaded into `rdi`, `rsi`, and `rdx`.

The initial context is sufficient to create the first user trap frame. The
SYSCALL entry path switches onto the current task's guarded kernel stack,
captures a runtime `UserTrapFrame`, and stores returning syscall frames on the
current user task. Interrupt capture still needs to save real runtime register
state before user preemption can be enabled.

## Full Trap Frame Layout

`kernel::task::context::UserTrapFrame` is the resume contract for user tasks.
Its field order is fixed by `#[repr(C)]` and compile-time offset
assertions:

| Offset | Field | Source |
| --- | --- | --- |
| `0` | `instruction_pointer` | hardware `iretq` frame `rip` |
| `8` | `code_segment` | hardware `iretq` frame `cs` |
| `16` | `cpu_flags` | hardware `iretq` frame `rflags` |
| `24` | `stack_pointer` | hardware `iretq` frame `rsp` |
| `32` | `stack_segment` | hardware `iretq` frame `ss` |
| `40` | `rax` | software save |
| `48` | `rbx` | software save |
| `56` | `rcx` | software save |
| `64` | `rdx` | software save |
| `72` | `rsi` | software save |
| `80` | `rdi` | software save |
| `88` | `rbp` | software save |
| `96` | `r8` | software save |
| `104` | `r9` | software save |
| `112` | `r10` | software save |
| `120` | `r11` | software save |
| `128` | `r12` | software save |
| `136` | `r13` | software save |
| `144` | `r14` | software save |
| `152` | `r15` | software save |

The total size is 160 bytes. `rsp` is intentionally not listed as a general
register because the user stack pointer is restored from the `iretq` frame.

## Interrupt Save Set

For interrupts taken from user mode, the architecture entry path must save:

- Hardware frame: `rip`, `cs`, `rflags`, `rsp`, `ss`.
- Caller-saved registers: `rax`, `rcx`, `rdx`, `rsi`, `rdi`, `r8`, `r9`,
  `r10`, and `r11`.
- Callee-saved registers: `rbx`, `rbp`, `r12`, `r13`, `r14`, and `r15`.

The timer interrupt must not schedule away from a user task until all of these
fields are captured in the owning task's `UserTrapFrame`.

## Syscall Save Set

The syscall path must preserve enough state to return through the syscall ABI:

- User return address and flags supplied by the architecture syscall entry.
- User stack pointer active at syscall entry.
- Syscall argument registers that user code may observe after return.
- All callee-saved registers.

ManaOS should store the same `UserTrapFrame` shape for syscalls and interrupts
so the scheduler does not need separate resume formats.

`kernel::interrupt::syscall_entry` now switches from the user stack to the
current task's guarded kernel stack, builds this frame, and calls the Rust
syscall dispatcher. The dispatcher writes the syscall result back to `rax`,
fills the user selectors, and records returning syscall frames in the current
user task metadata.

## Preemption Enablement Checklist

User task preemption stays disabled until all of the following are true:

- User-mode interrupt and syscall entries save a complete `UserTrapFrame`.
- User task metadata owns the saved trap frame and exposes it only through task
  lifecycle helpers.
- Per-task kernel stacks exist and are installed before entering or resuming a
  user task.
- Timer interrupt routing can distinguish user-mode frames from kernel-mode
  frames without depending on `kernel::task` from `arch/`.
- Page-fault diagnostics can report whether a fault happened in user or kernel
  mode.
- The scheduler can transition a user task from `Running` to `Ready` only after
  its trap frame is saved.
- `just storage-smoke` still proves the one-shot user path, and a dedicated
  preemption smoke proves a timer interrupt can resume user code.
