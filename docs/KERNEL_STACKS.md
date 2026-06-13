# ManaOS Kernel Stacks

This document defines the kernel stack guard-page design and the per-task
kernel stack switching policy.

## Current Stack Model

ManaOS currently has three kernel stack categories:

- Bootstrap stack: the stack active when the kernel enters from UEFI. It is not
  currently represented by `kernel::task`.
- Kernel task stacks: owned by `kernel::task::stack::KernelStack` metadata and
  currently backed by contiguous heap buffers with no unmapped guard page.
- User task kernel stacks: also owned by `KernelStack` metadata before Ring 3
  entry, and the one-shot user path installs the stack top in the x86_64 TSS
  through a registered architecture provider before entering Ring 3.
- Architecture stacks: `arch::x86_64::global_descriptor_table` owns a Ring 0
  privilege stack and a double-fault interrupt stack table entry in the TSS.
  Both are currently static byte arrays.

This is boot-safe for the current cooperative kernel tasks, but it is not the
right final shape for user preemption, nested traps, or stack overflow
diagnostics.

## Guard Page Placement

Kernel stacks should grow downward. Every owned kernel stack allocation should
reserve one unmapped guard page below the lowest writable stack page:

```text
high address
  writable stack pages
  ...
  writable stack page 0
  unmapped guard page
low address
```

Required placement rules:

- Guard pages are `Reserved` physical-frame ownership entries and must not be
  mapped into the active address space.
- Writable stack pages must be mapped `PRESENT | WRITABLE | NO_EXECUTE` and
  must not be `USER_ACCESSIBLE`.
- Stack top values must point one byte past the highest mapped stack byte and
  must remain 16-byte aligned before context entry.
- The guard page must be directly adjacent to the writable stack range so a
  downward overflow faults before corrupting another kernel object.
- The double-fault IST stack must have its own independent guard page and must
  never share pages with normal task stacks.

## Allocation Policy

Guarded kernel stacks require a kernel virtual memory allocator. Until that
exists, kernel task stacks must stay as heap-owned buffers and the guard-page
TODO must remain implementation-incomplete.

When dynamic kernel mappings exist, stack allocation should move into a focused
`kernel::task::stack` module:

- Allocate `N + 1` virtual pages where the lowest page is the guard page.
- Allocate `N` physical frames for writable stack pages.
- Map writable pages as kernel-only, writable, non-executable.
- Store stack metadata on the task: base, top, writable page count, and guard
  page virtual address.
- Freeing a stack is allowed only after the task is `Finished` and no scheduler,
  interrupt, or architecture context can still reference it.

## Per-Task Kernel Stack Switching Policy

User tasks need a kernel stack distinct from the bootstrap task stack before
preemptive user scheduling is safe.

Policy:

- Every schedulable task gets a kernel stack owner record.
- Kernel tasks enter with their own kernel stack in `TaskContext`.
- User tasks get a kernel stack used for syscall and interrupt handling.
- Before entering or resuming a user task, the scheduler must install that
  task's kernel stack top into the architecture task provider.
- On x86_64, installing the user task kernel stack means updating TSS
  `privilege_stack_table[0]` through an architecture-owned API registered from
  `main.rs`.
- `arch/` must not call `kernel::task` directly. `main.rs` remains the
  composition root for registering stack-switch providers.
- Timer interrupts must not preempt a user task into a shared bootstrap stack
  once user preemption is enabled.

## Fault Diagnostics

Page-fault diagnostics should classify kernel stack guard faults before generic
page-fault reporting:

- Fault address is inside a known kernel stack guard page: report task id,
  stack owner, guard page address, current stack pointer when available, and
  whether the fault came from kernel or user mode.
- Fault address is inside a writable kernel stack page: report stack overflow
  suspicion only if the stack pointer is outside the task's writable range.
- Fault address is outside any known stack: fall back to generic page-fault
  diagnostics.

Double-fault handling must remain minimal. It should report that the double
fault used the IST stack and should avoid taking locks that may already be held
by the faulting path.

## Implementation Order

1. Add kernel virtual address range allocation for stack mappings. This is
   complete for reservation-only ranges; page-table mapping integration is
   still pending.
2. Introduce `kernel::task::stack` metadata without changing scheduling.
   This is complete for heap-backed kernel and user task records.
3. Move kernel task stack allocation from heap-backed `KernelStack` to guarded
   mapped stacks.
4. Add an architecture provider for updating the x86_64 TSS Ring 0 stack.
   This is complete for the one-shot user entry path.
5. Give user tasks kernel stacks and install the stack before Ring 3 entry.
   This is complete for the one-shot user entry path.
6. Add page-fault diagnostics that detect known guard pages.
7. Enable user task preemption only after full user trap frames and per-task
   kernel stack switching are both verified.
