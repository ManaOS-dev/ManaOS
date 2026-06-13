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
  specific maximum length.
- Non-zero syscall pointer arguments are converted from raw `u64` ABI values to
  `UserVirtualRange`, then wrapped as `UserReadableRange` or
  `UserWritableRange` before the copy helpers run.
- NUL-terminated path arguments are additionally wrapped as `UserCString`
  before `copy_cstr_from_user` scans for the terminator.
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
| `mmap(addr, len, prot, flags)` | none | none | none | Anonymous private mappings only; `addr` must be zero, `len` must be non-zero, and executable mappings are rejected. |
| `munmap(addr, len)` | none | none | none | Exact anonymous mapping unmaps only; `addr` must be page-aligned and no user buffer is copied. |
| `exit(code)` / `exit_group(code)` | none | none | none | No user pointer validation. |
| `getpid()` | none | none | none | No user pointer validation. |

## Current Enforcement Gaps

- Kernel/user mapping permission self-checks should prove that kernel-only pages
  are not `USER_ACCESSIBLE` and user pages carry the expected readable/writable
  permissions.
- `UserReadableRange`, `UserWritableRange`, and `UserCString` encode syscall
  pointer intent, but page-table permission checks still happen inside the copy
  helpers.
- Execute permission is enforced for syscall data pointers by rejecting
  executable user pages. Future instruction-fetch page-fault checks should
  still report executable permission faults separately.

## Future Typed Pointer Split

The `user_pointer` module should eventually split terminated strings from
candidate string ranges:

- `ValidatedUserCString`

That type should be created only after finding a terminating NUL byte inside a
readable range.
