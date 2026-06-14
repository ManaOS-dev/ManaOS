# Process Lifecycle

ManaOS process lifecycle work is staged so syscall ABI, filesystem lookup,
ELF loading, address-space publication, descriptor inheritance, and scheduler
metadata can be verified one slice at a time.

## Ownership

- `kernel::syscall` owns syscall number dispatch, argument register decoding,
  errno-style result mapping, and syscall tracing.
- `kernel::memory::user_pointer` owns copying user pointers into kernel-owned
  staging data before lifecycle state is mutated.
- `kernel::filesystem` owns path normalization, namespace lookup, file
  metadata, descriptor tables, and filesystem error values.
- `kernel::elf` owns ELF validation and segment mapping policy for user images.
- `kernel::memory` owns user address-space construction, publication,
  rollback, and frame reclamation.
- `kernel::task` owns process identifiers, parent-child metadata, scheduler
  state, trap frames, exit records, and lifecycle diagnostics.
- `main.rs` remains the composition root for boot-time smoke wiring only; it
  must not become the owner of process replacement policy.

## Current Status

The current kernel can load user ELF images from the filesystem for the smoke
lifecycle, build initial `argc` / `argv` / `envp` stack state, run multiple
active user task records under timer preemption, retain parent-child metadata,
successfully replace a running smoke task image through `execve`, and reclaim
finished user address spaces and scheduler-owned kernel stacks.
The kernel-internal `kernel::process::spawn_user_program` helper now owns the
boot-visible path from filesystem executable path to initial user task record,
while filesystem lookup, ELF mapping, address-space construction, and scheduler
metadata remain owned by their existing modules.
`kernel::process::UserProgramEntryVectors` is the named pre-stack
representation for borrowed `argv` and `envp` slices used by spawned programs.
The spawn helper now classifies executable path lookup failures with stable
errno-facing results for missing, relative, directory, device, and invalid ELF
targets before a future user-visible spawn syscall is added.
Scheduler diagnostics retain the spawned origin path separately from the current
image path, so a later successful `execve` can change `path=` while `origin=`
still identifies the program that created the task record.

General user-created process lifecycle is not complete yet. Current working
directories are now task metadata, relative paths resolve through the current
task's directory, and successful `execve` preserves that directory across image
replacement. User-visible child creation and the scheduler-backed `waitpid`
wait/reap state machine are still future work. Descriptor close-on-exec metadata
and successful-`execve` close behavior exist for the current global descriptor
table. The `waitpid` syscall number, option constants, no-std userland wrapper,
selector validation, no-child `ECHILD` path, and scheduler-owned child exit
records keyed by parent task identifier are in place now so later child-exit
work has a stable ABI target.

## `waitpid` Syscall Contract

`waitpid` will let a parent process observe and reap exited child processes
without exposing scheduler-internal exit records to userland. ManaOS reserves
the Linux-compatible `wait4` syscall number as `SYS_WAITPID` and intentionally
starts with the narrower `waitpid` argument subset.

The syscall ABI slice uses the normal ManaOS syscall register convention:

- `rdi`: process identifier selector. A positive value matches that child
  process identifier. `WAIT_ANY` (`-1`) matches any child. Process-group
  selectors are not supported in the first subset.
- `rsi`: user pointer to a 32-bit wait status word. A null pointer is accepted
  and suppresses status storage.
- `rdx`: option bits. `0` means a blocking wait. `WNOHANG` returns immediately
  when no matching child has exited. Any other option bit should return
  `-EINVAL`.

Current kernel dispatch accepts `WAIT_ANY` and positive child process
identifiers, rejects unsupported option bits and process-group selectors with
`-EINVAL`, and returns `-ECHILD` when the current user task has no matching
child. If a matching child already has a waitable exit record, the syscall
collects that record, returns the child process identifier, and stores the
normal wait status word when the status pointer is non-null. `WNOHANG` returns
`0` when a matching child exists but no matching child exit is waitable yet.
Blocking wait still returns `-ENOSYS` until waiting parents can sleep and wake
through the scheduler. The syscall does not return `-EINTR` because ManaOS has
no documented user interrupt policy yet. Storage smoke covers the no-child and
explicit non-child selector paths through the no-std userland wrapper, plus the
scheduler-owned selected-child reap path, so later behavior changes are
explicit.

The remaining scheduler-backed contract is:

- Return the reaped child process identifier on success.
- Store normal process exit status as `(exit_code & 0xff) << 8` when the status
  pointer is non-null.
- Return `0` for `WNOHANG` when the caller has a matching child but no matching
  exited child is ready to reap.
- Preserve each child exit status until exactly one successful reap consumes it.
- Reclaim address-space and kernel-stack resources only after the exit record is
  safe according to the scheduler-owned lifecycle policy.

## Parent-Child Lifecycle States

The current scheduler already records the parent task identifier when a kernel
or user task is spawned. Successful `execve` keeps the same task identifier and
parent relationship, so image replacement does not create a new child from the
parent's point of view.

The current lifecycle states are:

- Running or ready child: the child task has a parent identifier, still owns any
  live user runtime resources, and is not waitable yet.
- Finished waitable child: `SYS_EXIT` moved the user task to `Finished`,
  retained its exit code in a parent-keyed child exit record, and made the
  status available to the recorded parent.
- Collected child: the parent-side collection path consumed the retained exit
  code once and marked the child exit record collected. A second collection for
  the same child returns no exit record.
- Reclaimed child resources: the scheduler-owned cleanup path released the
  finished child's user address space and kernel stack after the current smoke
  lifecycle no longer needs to resume the child.

Scheduler diagnostics expose `zombie_user_tasks` for finished children whose
exit status is still waitable and `reaped_user_tasks` for child exit records
already collected by their recorded parent. The older waitable/collected exit
status counters remain available for compatibility with existing smoke logs.
The `tasks` console command also prints a per-task `lifecycle` label, using
`waiting` for blocked tasks, `zombie` for uncollected child-exit records, and
`reaped` for collected child-exit records.

The future general process model must keep these invariants:

- A child is waitable only to its recorded parent, except for a documented
  reparenting policy after parent exit.
- A successful `execve` never changes process identifier, parent identifier, or
  waitability.
- A child exit status remains observable until exactly one successful parent
  reap consumes it.
- `waitpid(WNOHANG)` may return `0` only when the caller has a matching child
  but no exited matching child is ready to reap.
- Address-space and kernel-stack reclamation must not erase the exit status
  before the parent can reap it.
- Orphan handling must be explicit: either reparent to the documented initial
  process or reject the process model that can produce orphans.

## `execve` Kernel Contract

`execve` replaces the current process image while preserving the process
identity and lifecycle relationship that make the task observable to parents
and diagnostics.

The syscall ABI slice should use the normal ManaOS syscall register convention:

- `rdi`: user pointer to a NUL-terminated executable path.
- `rsi`: user pointer to a NUL-terminated `argv` pointer array.
- `rdx`: user pointer to a NUL-terminated `envp` pointer array.

The shared syscall number and no-std userland wrapper are implemented now. The
kernel stages the executable path, `argv`, and `envp` through user pointer
validation, resolves the executable through the current filesystem namespace,
validates ELF metadata, builds a replacement candidate, publishes the prepared
address space and trap frame through the scheduler, and reclaims the old user
image after the old instruction pointer can no longer resume.

The kernel-side contract is:

- Copy the executable path before opening or mutating process state.
- Copy `argv` and `envp` arrays through the user pointer validation helpers.
- Treat `argv == NULL` as an empty argument vector.
- Treat `envp == NULL` as an empty environment vector.
- Cap path bytes, argument count, environment count, and total copied
  argument/environment bytes with named constants before allocation or stack
  construction.
- Resolve the path through the current process filesystem namespace. Relative
  paths are interpreted against the task-owned current working directory.
- Reject directory targets with `-EISDIR`.
- Reject missing targets with `-ENOENT`.
- Reject unsupported device targets with `-EOPNOTSUPP`.
- Reject non-ELF or unsupported ELF images with `-EINVAL` unless a later ABI
  slice adds an executable-format errno.
- Reuse the existing user ELF validation and mapping policy; do not add an
  `execve`-specific ELF parser.
- Build a fresh user stack containing the copied `argv` and `envp` strings and
  pointer arrays.
- Preserve the current process identifier on success.
- Preserve the parent process identifier and waitable-child relationship on
  success.
- Preserve the task-owned current working directory across successful image
  replacement.
- Preserve open descriptors by default, then close only descriptors marked
  close-on-exec after the replacement image has been published.
- Reset runtime state that belongs to the old image, including saved user trap
  frames, syscall trace state scoped to the image, sleep/block state, pending
  user mapping records, heap break state, and executable mapping metadata.
- Publish the new address space only after the executable image, heap start,
  user stack, and initial trap frame are fully prepared.

Successful `execve` does not return to the old user instruction pointer. The
next user resume must enter the new image entry point with the new stack state.
Failure returns a negative errno to the old image and leaves the old process
image runnable.

## Argument And Environment Staging

`execve` must not walk user memory while partially installed process state is
visible. The safe sequence is:

1. Copy the path, pointer arrays, and pointed-to strings into bounded
   kernel-owned staging storage.
2. Validate the executable target and loadable ELF metadata.
3. Build a new address space, user mappings, heap start, and user stack from
   staged data.
4. Publish the prepared image in one scheduler-owned lifecycle transition.

The first implementation should keep limits close to the existing initial-entry
stack support: a small fixed argument count, a small fixed environment count,
and one-page total copied string storage. Increasing those limits later should
be a deliberate ABI and smoke-test change.

Current staging uses the existing 256-byte path cap, 8 `argv` entries, 8 `envp`
entries, and 4096 total copied argument/environment string bytes including NUL
terminators. Invalid user pointers return `-EFAULT`; count or byte limit
overflow returns `-E2BIG`.

Current path validation resolves absolute paths directly and relative paths
against the task-owned current working directory. It reads regular file
contents through a temporary descriptor, rejects missing paths with `-ENOENT`,
rejects directories with `-EISDIR`, rejects device nodes with `-EOPNOTSUPP`,
and rejects invalid ELF metadata with `-EINVAL`. Valid images are mapped into a
candidate address space with byte-preserving `argv` and `envp` stack contents,
then published by replacing the current task's address space, heap state,
private mapping state, and saved user trap frame.

## Address-Space Publication And Rollback

The old image remains authoritative until the new image is fully built. A
partially built address space must never be installed on the task record,
scheduled, or exposed through `tasks` diagnostics as active.

On failure, cleanup must release every resource allocated for the candidate
image:

- candidate user PML4 and page-table frames;
- candidate ELF segment frames;
- candidate user heap metadata and mapped heap frames;
- candidate private mapping records and frames;
- candidate user stack frames and guard reservations;
- any kernel staging buffers used for copied path, `argv`, or `envp` data;
- any descriptor references opened only for image loading.

The old address space, old trap frame, old user stack, old heap state, old
private mappings, current process ID, parent ID, and inherited descriptors must
remain unchanged on failure.

The current runtime path exercises successful publication: the kernel builds the
candidate address space, maps ELF segments, prepares the candidate user stack
and trap frame, swaps the task record through `kernel::task`, overwrites the
syscall stack trap frame with the new image entry state, and reclaims the old
address space through owner-checked frame allocator paths. Candidate
construction is still panic-on-OOM and must become fallible before this path is
used as a general process facility.

On success, the scheduler lifecycle transition owns the swap:

1. Replace the task's address-space root, heap bookkeeping, private mapping
   bookkeeping, sleep state, and initial resume trap frame.
2. Write the new image trap frame back to the syscall stack frame.
3. Return the internal successful `execve` sentinel so syscall dispatch does
   not write an old-image return value.
4. Mark old user memory and mapping records reclaimable only after no return
   path can resume the old image.
5. Reclaim old image resources through the same owner-checked frame allocator
   paths used by finished-task cleanup.
6. Record diagnostics for the old image reclaim and the new image publication.

## Descriptor Inheritance

Descriptors are inherited across successful `execve` unless the descriptor is
marked close-on-exec. Storage smoke now opens the executable file in the old
image as the first non-standard descriptor and verifies that the new image can
close the same descriptor number.

The current descriptor table records close-on-exec metadata per open file. The
user-visible `OPEN_CLOSE_ON_EXEC` flag marks a descriptor for successful
`execve` cleanup. Unmarked descriptors keep their descriptor numbers and offsets
by default, while marked descriptors are closed only after the new image is
ready to run.

The current table is still global rather than per-process, so this is the
minimum metadata needed for the smoke lifecycle. Future per-process descriptor
tables must preserve the same rule but apply it only to the execing process.

## Diagnostics And Smoke Coverage

Current runtime diagnostics cover the first successful replacement path:

- Storage smoke proves a successful self-replacement from `/disk/bin/smoke_demo`
  and verifies that the old image does not resume.
- Storage smoke changes the user task's current working directory to `/disk`,
  then proves relative self-`execve` and relative post-exec `file_demo`
  replacement resolve through that preserved directory.
- Storage smoke starts user programs through the kernel-internal
  `spawn_user_program` helper so filesystem path loading, ELF mapping, initial
  argv/envp stack construction, and scheduler task creation share one path.
- Storage smoke asserts the staged entry vector counts before the helper builds
  the initial user stack.
- Storage smoke asserts stable spawn path errno mappings for missing, relative,
  directory, device, and non-ELF targets before successful spawn task creation.
- Storage smoke asserts three distinct user tasks spawned from the same
  filesystem path before all are activated together.
- Storage smoke asserts that `tasks` output retains the original spawn path as
  `origin=` after the same task successfully replaces its current image through
  `execve`.
- Storage smoke verifies that an unmarked descriptor inherited through
  successful self-`execve` remains usable in the new image.
- Storage smoke asserts the kernel log emitted when descriptors opened with
  `OPEN_CLOSE_ON_EXEC` are closed during successful image replacement.
- Storage smoke verifies that replacement is not limited to self-`execve` by
  replacing the post-exec smoke image with `/disk/bin/file_demo`.
- Serial logs record `User image replaced by execve` and
  `execve image published` with old-image reclaim counts.
- Scheduler smoke verifies that `execve` resets heap and private mapping
  bookkeeping before the post-exec image exits.
- Storage smoke verifies that `waitpid` returns `-ECHILD` when the current user
  task has no child and when a positive process identifier is not its child.
- Storage smoke asserts the scheduler log line that retains a parent-keyed
  child exit record and the later line that collects that record once.
- Storage smoke asserts selected-child wait collection and zero-exit wait
  status encoding through the scheduler-owned child exit records.
- Storage smoke asserts a stable wait lifecycle summary showing retained child
  count, collected child count, and double-reap prevention.
- The `tasks` console command shows each user task's spawned origin path,
  current image generation, retained image path, and last successful old-image
  reclaim counts.

Remaining runtime diagnostics should cover broader behavior:

- `tasks` output should show replacement building and failed states once
  candidate construction has fallible post-build failure points.
- Failure smoke should prove any future post-candidate failure returns all
  candidate frames and keeps the old image runnable.

These diagnostics should use stable serial log lines so future CI smoke can
assert the process lifecycle without parsing interactive console output.
