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
current user task. The x86_64 timer interrupt entry also captures a complete
general-purpose register snapshot for Ring 3 timer frames and records it on the
current user task. The scheduler can now preempt a Ring 3 task from the timer
path, run another schedulable task, and resume the preempted user task through
the saved timer interrupt context. User tasks also carry a `UserAddressSpace`
root; the lifecycle path switches CR3 before Ring 3 entry and restores the
kernel address space after `SYS_EXIT`.

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
The timer path now enters through an assembly stub that saves this full
general-purpose register set before calling the Rust architecture hook. The
architecture hook acknowledges the PIC and passes a shared timer frame to
`kernel::interrupt`, which records Ring 3 timer frames in task metadata without
making `arch/` depend on `kernel/`.

After the frame is recorded, `kernel::task` may switch away from the user task.
The switch stores the interrupted task's kernel-side timer context in its
`TaskContext`, so scheduling that task again returns to the timer handler and
then `iretq` resumes user code. Initial user entries use a separate
`switch_to_user_mode` architecture provider that saves the current task context
before consuming a `UserTrapFrame`. When the scheduler resumes a saved user
timer context, it switches to the saved task's user address space before
restoring that context.

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
  lifecycle helpers. This is complete for returning syscalls and the current
  PIT timer interrupt path.
- Per-task kernel stacks exist and are installed before entering or resuming a
  user task. This is complete for first entry, syscall entry, and timer-context
  resume on the current scheduler path.
- User tasks own separate page-table roots for ELF and stack mappings. This is
  complete for the current one-shot smoke path.
- Timer interrupt routing can distinguish user-mode frames from kernel-mode
  frames without depending on `kernel::task` from `arch/`. This is complete for
  the current PIT timer path.
- Page-fault diagnostics can report whether a fault happened in user or kernel
  mode.
- The scheduler can transition a user task from `Running` to `Ready` only after
  its trap frame is saved. This is complete for timer-driven preemption.
- Scheduler diagnostics expose task state counts, user address-space ownership,
  and preemption accounting so the boot smoke can assert lifecycle progress.
- The `tasks` console command shows the same scheduler and preemption counters
  on the interactive console overlay, then lists one row per retained task with
  kind, lifecycle state, active scheduling membership, user address-space
  ownership, and scheduler-managed kernel stack ownership.
- The console overlay status strip now keeps the scheduler and preemption
  counters visible even before a command is submitted.
- `just storage-smoke` still proves the one-shot user path and now asserts that
  timer interrupts can enter another active user task, preempt and resume user
  code across two user task records, and finish tasks that own separate stack
  slots, separate address spaces, and lifecycle diagnostics. Finished user
  exits are now reported through a scheduler-owned exit queue instead of a
  global single-result latch, so lifecycle cleanup can drain task-specific exit
  records before asking the scheduler for one aggregate resource-reclaim pass
  across address spaces and kernel stacks. The one-shot `SYS_EXIT` return stack
  is guarded by an explicit return window that must be set and consumed exactly
  once. `SYS_EXIT` closes scheduler preemption before returning through that
  one-shot stack, so another active user task cannot consume the same return
  window while lifecycle cleanup is still pending. Scheduler diagnostics now
  expose both the explicit preemption state (`enabled`, `disabled`, or
  `user_exit_return`) and the number of user exits that closed this window. The
  smoke lifecycle asks the scheduler for the next active user task instead of
  selecting task identifiers in the composition root, so active-set ownership
  stays inside `kernel::task`. The active user lifecycle can now be drained
  through one scheduler-owned API that returns the completed exit records.
