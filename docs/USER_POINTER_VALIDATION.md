# ManaOS User Pointer Validation

This document defines the syscall-by-syscall user pointer validation policy.
The implementation entry point is `kernel::memory::user_pointer`.

## Common Rules

- A zero-length buffer is valid and does not require a mapped user pointer.
- Non-zero user pointers must be non-null and below the canonical user-space
  limit `0x0000_8000_0000_0000`.
- Pointer plus length must not overflow.
- Input buffers must use `copy_from_user`, which requires a present,
  user-accessible, non-executable mapping.
- Output buffers must use `copy_to_user`, which requires a present writable
  user-accessible, non-executable mapping.
- NUL-terminated path strings must use `copy_cstr_from_user` with a syscall
  specific maximum length. The helper validates each byte up to the first NUL
  terminator, so a short string near the end of a mapped user page is accepted
  as long as the terminator itself is readable.
- NUL-terminated pointer arrays must copy each pointer slot with
  `copy_from_user` before scanning the pointed-to strings.
- Non-zero syscall pointer arguments are converted from raw `u64` ABI values to
  `UserVirtualRange`, then wrapped as `UserReadableRange` or
  `UserWritableRange` before the copy helpers run.
- Page-table permission probes consume those copy-direction wrappers. Raw
  `usize` pointers are used only when creating the final kernel slice or reading
  one already-classified user byte.
- User address-space permission self-checks keep representative kernel probes as
  `VirtAddr` and representative user probes as `UserVirtualAddress` before
  forming the single-byte readable or writable ranges used by the probe.
- NUL-terminated path arguments are additionally wrapped as `UserCString`
  before `copy_cstr_from_user` scans readable bytes for the terminator.
- Syscalls return Linux-like `-EFAULT` (`ERROR_BAD_ADDRESS`) when pointer
  validation fails.

## Syscall Policy

| Syscall | Pointer argument | Direction | Required helper | Extra policy |
| --- | --- | --- | --- | --- |
| `write(fd, buf, len)` | `buf` | user to kernel | `copy_from_user` | `len` must fit in `usize`; zero length is allowed. |
| `read(fd, buf, len)` | `buf` | kernel to user | `copy_to_user` | `len` must fit in `usize`; zero length is allowed. |
| `open(path, flags, mode)` | `path` | user to kernel | `copy_cstr_from_user` | Path is capped by `MAX_USER_STRING_LENGTH`. |
| `openat(dirfd, path, flags, mode)` | `path` | user to kernel | `copy_cstr_from_user` through `sys_open` | Only `AT_FDCWD` is supported today. |
| `fstat(fd, statbuf)` | `statbuf` | kernel to user | `copy_to_user` | Buffer size is exactly `UserFileStat`. |
| `getdents64(fd, dirp, count)` | `dirp` | kernel to user | `copy_to_user` | `count` must fit in `usize` and be at least one `UserDirectoryEntry`. |
| `close(fd)` | none | none | none | No user pointer validation. |
| `lseek(fd, offset, whence)` | none | none | none | No user pointer validation. |
| `brk(addr)` | none | none | none | The address must stay within the task heap range; no user buffer is copied. |
| `mmap(addr, len, prot, flags, fd, offset)` | none | none | none | Private mappings only; automatic anonymous mappings use `addr = 0`, fixed mappings require a page-aligned non-zero address, `MAP_FIXED_NOREPLACE` rejects overlaps, `MAP_FIXED` replaces private mapping records, file-private mappings require a regular file descriptor and page-aligned offset, and executable mappings are rejected. |
| `munmap(addr, len)` | none | none | none | Private mapping range unmap only; `addr` must be page-aligned, the range must stay inside tracked private mapping records, and no user buffer is copied. |
| `nanosleep(req, rem)` | `req`, optional `rem` | user to kernel, kernel to user | `copy_from_user`, `copy_to_user` | `req` is exactly `UserTimespec`; non-zero `rem` is exactly `UserTimespec` and is zero-filled because signal interruption is not implemented. |
| `execve(path, argv, envp)` | `path`, `argv`, `envp` | user to kernel | `copy_cstr_from_user`, `copy_from_user` | Path is capped by `MAX_USER_STRING_LENGTH`. `argv == NULL` and `envp == NULL` are accepted as empty vectors. Argument and environment vectors are capped at 8 entries each and 4096 total copied string bytes including NUL terminators. Invalid pointers return `-EFAULT`; limit overflow returns `-E2BIG`. A valid ELF image replaces the current user image and does not return to the old instruction pointer. |
| `spawn(path, argv, envp)` | `path`, `argv`, `envp` | user to kernel | `copy_cstr_from_user`, `copy_from_user` | Path is capped by `MAX_USER_STRING_LENGTH`. `argv == NULL` or an empty vector uses the resolved path as the default `argv[0]`; `envp == NULL` is accepted as an empty vector. Argument and environment vectors use the same 8-entry and 4096-byte copied string limits as `execve`. Invalid pointers return `-EFAULT`; limit overflow returns `-E2BIG`. |
| `waitpid(pid, status, options)` | optional `status` | kernel to user | `copy_to_user` | A null status pointer is accepted. Non-null status pointers must reference exactly a writable 32-bit wait status word. Blocking waits validate the pointer before sleeping, then write the status after switching back to the waiting parent's address space. |
| `exit(code)` / `exit_group(code)` | none | none | none | No user pointer validation. |
| `getpid()` / `getppid()` | none | none | none | No user pointer validation. |

## Smoke Coverage

The storage smoke user program verifies representative syscall errno paths:
missing paths, bad file descriptors, unsupported `openat`, invalid
`getdents64`, invalid `mmap`/`munmap`, invalid `nanosleep`, and unmapped
`nanosleep` pointers. It also verifies `execve` validation for valid
`argv`/`envp`, bad pointer arrays, argument-count overflow, missing paths,
directory targets, non-ELF files, and a no-return successful self-`execve`.
The userland spawn smoke passes explicit `argv`/`envp` vectors and validates
them in the spawned child image.
The userland wait smoke validates pending `WNOHANG`, blocking `WAIT_ANY`, and
nonzero status storage through the waiting parent's address space.

## Current Enforcement Gaps

- Kernel/user mapping permission self-checks should prove that kernel-only pages
  are not `USER_ACCESSIBLE` and user pages carry the expected readable/writable
  permissions.
- `UserCString` still represents a candidate readable range until a terminating
  NUL byte is found; it is not yet a fully validated C-string type.
- Execute permission is enforced for syscall data pointers by rejecting
  executable user pages. Future instruction-fetch page-fault checks should
  still report executable permission faults separately.

## Future Typed Pointer Split

The `user_pointer` module should eventually split terminated strings from
candidate string ranges:

- `ValidatedUserCString`

That type should be created only after finding a terminating NUL byte inside a
readable range.
