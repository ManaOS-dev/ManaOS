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
and reclaim finished user address spaces and scheduler-owned kernel stacks.

General user-created process lifecycle is not complete yet. `execve`,
user-visible child creation, `waitpid`, process-owned current directories, and
close-on-exec descriptor metadata are still future work.

## `execve` Kernel Contract

`execve` replaces the current process image while preserving the process
identity and lifecycle relationship that make the task observable to parents
and diagnostics.

The syscall ABI slice should use the normal ManaOS syscall register convention:

- `rdi`: user pointer to a NUL-terminated executable path.
- `rsi`: user pointer to a NUL-terminated `argv` pointer array.
- `rdx`: user pointer to a NUL-terminated `envp` pointer array.

The shared syscall number and no-std userland wrapper are reserved now. The
kernel also stages the executable path, `argv`, and `envp` through user pointer
validation, resolves the executable through the current filesystem namespace,
validates ELF metadata, builds an unpublished replacement candidate, and rolls
that candidate back before returning the current unsupported runtime result.
The successful image publication path remains pending.

The kernel-side contract is:

- Copy the executable path before opening or mutating process state.
- Copy `argv` and `envp` arrays through the user pointer validation helpers.
- Treat `argv == NULL` as an empty argument vector.
- Treat `envp == NULL` as an empty environment vector.
- Cap path bytes, argument count, environment count, and total copied
  argument/environment bytes with named constants before allocation or stack
  construction.
- Resolve the path through the current process filesystem namespace. Until
  process-owned current directories exist, only absolute paths should be
  accepted for user `execve`.
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
- Preserve the current working directory once current directories are owned by
  process metadata.
- Preserve open descriptors by default, then close only descriptors marked
  close-on-exec after descriptor metadata supports that flag.
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

Current path validation accepts only absolute executable paths, reads regular
file contents through a temporary descriptor, rejects missing paths with
`-ENOENT`, rejects directories with `-EISDIR`, rejects device nodes with
`-EOPNOTSUPP`, and rejects invalid ELF metadata with `-EINVAL`. Valid images
are mapped into an unpublished candidate address space with byte-preserving
`argv` and `envp` stack contents, then rolled back while `execve` still returns
`-ENOSYS`.

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

The current unsupported valid-image path already exercises this rule: the
kernel builds the candidate address space, maps ELF segments, prepares the
candidate user stack and trap frame, destroys the candidate address space, and
asserts that frame-owner totals match their pre-build snapshot before returning
`-ENOSYS` to the old image.

On success, the scheduler lifecycle transition owns the swap:

1. Close preemption for the current user task while replacement is committed.
2. Replace the task's address-space root and initial resume trap frame.
3. Mark old user memory and mapping records reclaimable only after no return
   path can resume the old image.
4. Reclaim old image resources through the same owner-checked frame allocator
   paths used by finished-task cleanup.
5. Record diagnostics for the old image reclaim and the new image publication.

## Descriptor Inheritance

Descriptors are inherited across successful `execve` unless the descriptor is
marked close-on-exec. The close-on-exec flag is not present yet, so the first
descriptor implementation step must add metadata without changing existing
descriptor numbers or offsets.

When close-on-exec exists, closing must happen only after the new image is
ready to publish. If closing a descriptor fails internally, the kernel should
panic with context rather than leave a partially replaced process with ambiguous
descriptor state.

## Diagnostics And Smoke Coverage

The first runtime implementation should add diagnostics before broad behavior:

- `tasks` output should show the last successful image path, current image
  generation, and whether an `execve` replacement is building, active, or
  failed.
- Storage smoke should prove a successful replacement from `/disk`.
- Failure smoke should prove missing-path and directory-target errors.
- Address-space smoke should prove failed replacement returns all candidate
  frames and keeps the old image runnable.

These diagnostics should use stable serial log lines so future CI smoke can
assert the process lifecycle without parsing interactive console output.
