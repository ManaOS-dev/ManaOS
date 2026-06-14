# Virtual Filesystem

ManaOS treats storage as layered components. AHCI exposes block devices, GPT
selects partitions, FAT32 provides a filesystem backend, and the virtual
filesystem gives the kernel console and syscalls a stable path and descriptor
surface.

## Ownership

- `kernel::driver::storage` owns block-device registration and sector I/O.
- GPT and FAT32 parsers own on-disk structure validation.
- `kernel::filesystem` owns mount points, canonical paths, file descriptors,
  directory handles, and errno-facing filesystem errors.
- Console commands and syscalls consume filesystem APIs; they should not parse
  FAT32 or partition structures directly.

## Path Normalization

The kernel virtual filesystem stores paths in a canonical absolute form.

- Repeated slashes are collapsed.
- `.` path components are ignored.
- `..` removes the previous component and never escapes above `/`.
- Trailing slashes do not create a different path.
- An empty normalized result is represented as `/`.

Examples:

- `/dev//console` becomes `/dev/console`
- `/disk/../README` becomes `/README`
- `/dev/` becomes `/dev`

The console resolves relative paths against its current working directory before
passing them to the virtual filesystem.

Normalization is part of the security boundary. `..` must not escape above `/`,
and a normalized path must mean the same thing for console commands, syscalls,
and future user shell execution.

## Mount Model

The VFS keeps an explicit mount table. Each mount point has:

- a canonical absolute mount path,
- a backend implementation,
- read-only or writable flags,
- directory traversal behavior,
- metadata and descriptor operations exposed through the common filesystem
  surface.

Mount flags are checked before mutating operations. FAT32 currently remains a
read-only backend even though lower-level AHCI writes exist.

## FAT32 Backend

FAT32 files mounted under `/disk` are read-only backend files. The virtual
filesystem stores metadata and a read callback; file bytes are fetched through
the storage subsystem when the file descriptor is read instead of being copied
into a heap buffer during boot.

The FAT32 backend is responsible for:

- validating the boot sector and FSInfo data,
- reading directory entries including long file names,
- following cluster chains across directory and file reads,
- rejecting invalid clusters and cluster-chain loops,
- exposing file metadata in the VFS format,
- mapping backend failures to filesystem errors.

## File Descriptors And Directories

File descriptors own a current offset and backend-specific open state. Regular
file reads advance the offset. `lseek` updates the offset according to the
validated seek mode. Directory handles use `getdents64`-style iteration and must
preserve enough offset state to resume listing without duplicating entries.

The descriptor layer should keep syscall errno mapping centralized. Backend
errors should not leak as ad hoc console strings or storage-driver booleans.

## Mutation Policy

Write-capable FAT32 support must be added as a separate, verified step. Before
mutating disk images, document and implement:

- transaction boundaries for directory entry and FAT updates,
- rollback behavior for partial allocation failures,
- flush or write-through semantics for modified FAT sectors,
- corruption assumptions while journaling is absent,
- smoke coverage that creates, reads, and deletes files on the QEMU disk image.

Until that policy is implemented, VFS write attempts against read-only mounts
must fail consistently.
