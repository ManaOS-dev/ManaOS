# ManaOS Kernel Stacks

This document defines the kernel stack guard-page design and the per-task
kernel stack switching policy.

## Current Stack Model

ManaOS currently has four kernel stack categories:

- Bootstrap stack: the stack active when the kernel enters from UEFI. It is not
  currently represented by `kernel::task`.
- Kernel task stacks: owned by `kernel::task::stack::KernelStack` metadata and
  backed by higher-half writable stack mappings with an unmapped guard page
  below them.
- User task kernel stacks: also owned by `KernelStack` metadata before Ring 3
  entry. The metadata owns a mapped writable stack range plus an unmapped guard
  page, and the one-shot user path installs that mapped stack top in the
  x86_64 TSS through a registered architecture provider before entering Ring 3.
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

Guarded kernel stacks require a kernel virtual memory allocator and page-table
mapping support. Scheduler-owned kernel and user task stacks now reserve a
higher-half virtual range, leave its lowest page unmapped as the guard page,
allocate physical frames for writable pages, and map those pages
`PRESENT | WRITABLE | NO_EXECUTE` without `USER_ACCESSIBLE`.
Finished user tasks reclaim those scheduler-owned stack resources after
`SYS_EXIT`, once execution has returned to the kernel address space and the
scheduler has marked the task `Finished`. Scheduler diagnostics keep reclaim
accounting for finished user stacks as part of aggregate finished-user resource
records, and the console overlay renders those counts alongside the task status
strip.

The bootstrap stack and architecture-owned TSS/IST stacks are still separate
static stack categories and do not yet use this allocation path.

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
- User tasks now own separate address spaces for ELF and stack mappings. The
  smoke path still uses separate user stack virtual slots until fixed stack
  address reuse is introduced.
- Before entering or resuming a user task, the scheduler must install that
  task's kernel stack top into the architecture task provider and switch CR3 to
  the task's user address space. The scheduler records that handoff in retained
  task snapshots so smoke can assert every finished user task had a nonzero
  resume handoff, address-space root, and kernel stack top.
- The scheduler keeps the selected kernel stack top as `VirtAddr` through the
  user-entry and timer-resume handoff paths. The task architecture facade also
  accepts that value as `VirtAddr` and lowers it to raw `u64` only when
  invoking the registered architecture stack installer. The separate `SYSCALL`
  entry stack-top atomic remains an ABI-facing raw lowering boundary.
- On x86_64, installing the user task kernel stack means updating TSS
  `privilege_stack_table[0]` through an architecture-owned API registered from
  `main.rs`.
- Ring 3 interrupt entries use the installed TSS privilege stack, so timer
  interrupts taken from user mode arrive on the current task's guarded kernel
  stack.
- `SYSCALL` does not use the TSS privilege stack automatically, so its entry
  path uses the current task's installed stack top to switch from the user stack
  to the guarded kernel stack before dispatching.
- `arch/` must not call `kernel::task` directly. `main.rs` remains the
  composition root for registering stack-switch providers.
- Timer interrupts must not preempt a user task into a shared bootstrap stack
  once user preemption is enabled. The returnable user path blocks the bootstrap
  task while its user return window is live, so timer preemption chooses
  a schedulable kernel task context or another active user task context instead
  of a stale bootstrap context. The return window stores and consumes its return
  stack exactly once before lifecycle cleanup.

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

Scheduler-owned kernel and user task guard pages are now classified by
`kernel::task` and reported by the page-fault diagnostic path before the
generic fault line. Bootstrap and architecture-owned IST stack guard
classification remains pending because those stacks are not yet represented by
`kernel::task`.

## Implementation Order

1. Add kernel virtual address range allocation for stack mappings. This is
   complete for scheduler-owned task stacks, including writable page-table
   mappings. Generic kernel range unmap/free support now exists, but scheduler
   stack destruction remains tied to future task lifecycle cleanup.
2. Introduce `kernel::task::stack` metadata without changing scheduling.
   This is complete for scheduler-owned kernel and user task records.
3. Move kernel task stack allocation from heap-backed `KernelStack` to guarded
   mapped stacks. This is complete for scheduler-owned kernel and user task
   kernel stacks; bootstrap and architecture-owned IST stacks remain pending.
4. Add an architecture provider for updating the x86_64 TSS Ring 0 stack.
   This is complete for the one-shot user entry path.
5. Give user tasks kernel stacks and install the stack before Ring 3 entry.
   This is complete for the one-shot user entry path.
6. Add page-fault diagnostics that detect known guard pages. This is complete
   for scheduler-owned kernel and user task stacks; bootstrap and
   architecture-owned IST stacks remain pending.
7. Enable user task preemption only after full user trap frames and per-task
   kernel stack switching are both verified. This is complete for x86_64 PIT
   timer preemption and resume across the current two-task user smoke path.
8. Attach user address spaces to user task records and switch CR3 before Ring 3
   entry or timer-context resume. This is complete for the current one-shot and
   timer-resume smoke paths, including finished-task address-space destruction,
   resume handoff diagnostics, and scheduler diagnostics for retained task
   records.
9. Reclaim finished user task kernel stacks after `SYS_EXIT`. This is complete
   for scheduler-owned user task stacks; bootstrap, kernel task, and
   architecture-owned stacks remain outside this lifecycle path.
10. Expose finished user task kernel stack reclaim accounting through scheduler
    diagnostics and the console overlay. This is complete for stack count,
    writable page count, and guard-inclusive virtual page count.
