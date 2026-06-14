# ManaOS User Pointer Validation

この文書は [`../USER_POINTER_VALIDATION.md`](../USER_POINTER_VALIDATION.md) の日本語版です。
syscall ごとの user pointer validation policy を定義します。実装の入口は
`kernel::memory::user_pointer` です。

## 共通ルール

- zero-length buffer は valid であり、mapped user pointer を必要としません。
- non-zero user pointer は non-null で、canonical user-space limit
  `0x0000_8000_0000_0000` 未満でなければなりません。
- pointer + length は overflow してはいけません。
- input buffer は `copy_from_user` を使います。present、user-accessible、non-executable mapping が必要です。
- output buffer は `copy_to_user` を使います。present、writable、user-accessible、
  non-executable mapping が必要です。
- NUL-terminated path string は syscall-specific maximum length 付きの
  `copy_cstr_from_user` を使います。
- NUL-terminated pointer array は、指している string を読む前に各 pointer slot を
  `copy_from_user` でコピーします。
- non-zero syscall pointer argument は raw `u64` ABI value から `UserVirtualRange` へ変換し、
  その後 `UserReadableRange` または `UserWritableRange` で copy direction を表現します。
- NUL-terminated path argument は、`copy_cstr_from_user` が terminator を探す前に
  `UserCString` で包みます。
- pointer validation に失敗した syscall は Linux-like `-EFAULT` (`ERROR_BAD_ADDRESS`) を返します。

## syscall policy

| Syscall | Pointer argument | Direction | Required helper | Extra policy |
| --- | --- | --- | --- | --- |
| `write(fd, buf, len)` | `buf` | user to kernel | `copy_from_user` | `len` は `usize` に収まる必要があります。zero length は許可します。 |
| `read(fd, buf, len)` | `buf` | kernel to user | `copy_to_user` | `len` は `usize` に収まる必要があります。zero length は許可します。 |
| `open(path, flags, mode)` | `path` | user to kernel | `copy_cstr_from_user` | path は `MAX_USER_STRING_LENGTH` で capped されます。 |
| `openat(dirfd, path, flags, mode)` | `path` | user to kernel | `copy_cstr_from_user` through `sys_open` | 現在は `AT_FDCWD` だけを support します。 |
| `fstat(fd, statbuf)` | `statbuf` | kernel to user | `copy_to_user` | buffer size は `UserFileStat` と完全一致します。 |
| `getdents64(fd, dirp, count)` | `dirp` | kernel to user | `copy_to_user` | `count` は `usize` に収まり、最低1つの `UserDirectoryEntry` 以上です。 |
| `close(fd)` | none | none | none | user pointer validation は不要です。 |
| `lseek(fd, offset, whence)` | none | none | none | user pointer validation は不要です。 |
| `brk(addr)` | none | none | none | address は task heap range 内に留まる必要があります。user buffer は copy しません。 |
| `mmap(addr, len, prot, flags, fd, offset)` | none | none | none | private mapping のみ。automatic anonymous mapping は `addr = 0`、fixed mapping は page-aligned non-zero address、`MAP_FIXED_NOREPLACE` は overlap を拒否、`MAP_FIXED` は private mapping record を置換、file-private mapping は regular file descriptor と page-aligned offset が必要、executable mapping は拒否します。 |
| `munmap(addr, len)` | none | none | none | tracked private mapping record 内の page-aligned range だけを unmap します。user buffer は copy しません。 |
| `nanosleep(req, rem)` | `req`, optional `rem` | user to kernel, kernel to user | `copy_from_user`, `copy_to_user` | `req` は `UserTimespec` と完全一致します。non-zero `rem` も `UserTimespec` で、signal interruption 未実装のため zero-fill します。 |
| `execve(path, argv, envp)` | `path`, `argv`, `envp` | user to kernel | `copy_cstr_from_user`, `copy_from_user` | path は `MAX_USER_STRING_LENGTH` で capped されます。`argv == NULL` と `envp == NULL` は empty vector として扱います。argument と environment vector はそれぞれ 8 entries、NUL terminator を含む copied string bytes は合計 4096 bytes で capped されます。invalid pointer は `-EFAULT`、limit overflow は `-E2BIG` を返します。runtime image replacement はまだ unsupported です。 |
| `exit(code)` / `exit_group(code)` | none | none | none | user pointer validation は不要です。 |
| `getpid()` / `getppid()` | none | none | none | user pointer validation は不要です。 |

## smoke coverage

storage smoke の user program は、代表的な syscall errno path を確認します。

- missing path。
- bad file descriptor。
- unsupported `openat`。
- invalid `getdents64`。
- invalid `mmap` / `munmap`。
- invalid `nanosleep`。
- unmapped `nanosleep` pointer。
- `execve` validation の valid `argv` / `envp`、bad pointer array、argument-count overflow、
  missing path、directory target、non-ELF file。

## 現在の enforcement gap

- kernel-only page が `USER_ACCESSIBLE` ではなく、user page が期待する readable/writable permission を
  持つことを、kernel/user mapping permission self-check でさらに証明する必要があります。
- `UserReadableRange`、`UserWritableRange`、`UserCString` は syscall pointer intent を表現しますが、
  page-table permission check は copy helper 内で行われます。
- syscall data pointer に対して executable user page を拒否することで execute permission を
  enforcement しています。将来の instruction-fetch page fault では executable permission fault を
  別途診断する必要があります。

## future typed pointer split

`user_pointer` module は、将来的に terminated string と candidate string range を分けるべきです。

- `ValidatedUserCString`

この型は、readable range 内で terminating NUL byte が見つかった後にだけ作成します。
