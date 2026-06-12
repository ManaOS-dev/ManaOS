# ManaOS Address Boundaries

This document inventories raw address APIs and defines where ManaOS should
introduce typed physical and virtual address wrappers.

## Address Type Boundary

ManaOS still mixes raw `u64`, `usize`, pointers, project-owned address
wrappers, and `x86_64` crate `PhysAddr` / `VirtAddr` values. The migration
keeps ABI-facing integers at the edges and introduces typed addresses at
kernel ownership boundaries:

- Syscall ABI arguments remain raw `u64` at `kernel::syscall::dispatch`, then
  convert to user pointer types or scalar values before validation.
- ELF file fields remain parsed as raw integer values, then convert to user
  virtual addresses before mapping.
- UEFI memory map physical starts remain raw at the boot boundary, then convert
  to physical frame/range types inside memory management.
- AHCI register programming splits `DmaPhysicalAddress` values into low/high
  register halves only at the device register boundary.
- Kernel virtual pointers should be created only after a mapping helper proves
  the target range is mapped in the active address space.

## Implemented Address Boundaries

The following boundaries now use project-owned address wrappers instead of
untyped cross-domain `u64` values:

- `kernel::memory::address::PhysAddr` represents raw physical byte addresses.
- `kernel::memory::address::VirtAddr` represents raw virtual byte addresses
  for internal arithmetic that must not mix with physical addresses.
- `PhysicalFrameStart` and `PhysicalFrameRange` represent allocatable 4 KiB
  frame starts and contiguous frame ownership.
- `DmaPhysicalAddress` represents physical addresses that may be programmed
  into AHCI command headers, received-FIS buffers, command tables, and PRDT
  entries.
- `UserVirtualAddress` and `UserVirtualRange` represent non-null user virtual
  addresses and byte ranges before syscall copy validation.
- `UserReadableRange`, `UserWritableRange`, and `UserCString` represent syscall
  copy direction and string policy before `copy_from_user`, `copy_to_user`, and
  `copy_cstr_from_user`.
- `user_stack::allocate_and_map_user_page(...) -> PhysicalFrameStart` now
  returns a typed physical frame start instead of a raw physical `u64`.
- `user_stack::map_user_range(...)` now accepts `UserVirtualAddress` and
  `PhysicalFrameStart` internally instead of crossing virtual and physical
  domains with raw `u64` parameters.
- `paging::map_kernel_mmio_range(...)` now accepts `PhysAddr` for the MMIO
  physical base address.
- `AhciDmaBuffers` stores `DmaPhysicalAddress` fields internally, and
  `dma::split_address(...)` accepts `DmaPhysicalAddress`.
- `StorageDataAddress` represents the active DMA data buffer used by
  `BlockDevice`, AHCI service helpers, GPT parsing, and FAT32 parsing.

## Remaining Raw Address API Inventory

The following APIs currently expose raw physical or virtual addresses across
module boundaries and should be typed before reusable frame allocation,
per-process page tables, or dynamic kernel mappings become general-purpose.

### Boot And Composition Root

- `src/main.rs`
  - `allocate_backbuffer(...) -> *mut u8` converts a physical frame allocation
    into a pointer under the identity-mapping assumption.
  - `arch::init(kernel::interrupt::syscall_entry as *const () as u64)` passes a
    function address as a raw architecture argument.
  - `run_user_smoke_demo(...)` passes user entry, user stack, `argv`, and `envp`
    addresses as raw `u64`.

### Frame Allocation And Heap

- `kernel::memory::frame_allocator::BumpFrameAllocator::add_region(start, pages)`
  and `reserve_region*` accept raw physical start addresses from the UEFI
  memory map and boot reservations.
- `BumpFrameAllocator::allocate_frame() -> Option<PhysicalFrameStart>` returns
  a typed 4 KiB-aligned physical frame start.
- `BumpFrameAllocator::allocate_frames(n) -> Option<PhysicalFrameRange>`
  returns the typed physical start and page count of a contiguous frame range.
- `kernel::memory::heap::init(heap_range: PhysicalFrameRange)` accepts a typed
  physical frame range that is also used as a virtual range while identity
  mapping is active.

### Paging And Framebuffer

- `kernel::memory::paging::init(..., framebuffer_base: u64, framebuffer_size:
  u64)` accepts a raw framebuffer physical range.
- Internal page-table helpers convert raw `u64` values into `PhysAddr` /
  `VirtAddr` locally; those conversions should move behind typed range
  wrappers.

### User Memory

- `kernel::memory::user_stack::allocate_user_stack(..., pages) ->
  UserVirtualAddress` returns a typed user virtual stack top.
- `PreparedUserStack` exposes typed user virtual `stack_pointer`,
  `argument_values_pointer`, and `environment_values_pointer`.
- `kernel::memory::user_pointer::copy_from_user` accepts
  `UserReadableRange`, and `copy_to_user` accepts `UserWritableRange`; syscall
  helpers convert raw ABI arguments first.
- `kernel::memory::user_pointer::copy_cstr_from_user` accepts `UserCString`,
  which wraps a readable range capped by the syscall path-length policy.

### ELF Loading

- `kernel::elf::LoadedElf::entry_point() -> UserVirtualAddress` exposes a typed
  user virtual entry point.
- `ProgramHeader::virtual_address() -> u64` remains raw because it exposes a
  field parsed directly from the ELF file. Loader validation converts accepted
  segment starts to `UserVirtualAddress` before mapping.

### Storage And AHCI DMA

- The storage parser and block-device path now uses `StorageDataAddress`. Raw
  pointer conversion is limited to sector-slice creation after the block device
  fills the active DMA data buffer.

## Recommended Wrapper Types

Continue introducing wrappers in small steps:

- `PhysAddr` for physical byte addresses. This now exists in
  `kernel::memory::address`.
- `VirtAddr` for virtual byte addresses. This now exists in
  `kernel::memory::address`.
- `PhysicalFrameStart` for 4 KiB-aligned physical frame starts.
- `PhysicalFrameRange` for frame start plus page count. This is now the return
  type for contiguous bump allocations.
- `KernelVirtualAddress` for mapped kernel virtual addresses.
- `UserVirtualAddress` for non-null user pointers and ELF virtual addresses.
  This now covers loaded ELF entry points, prepared user stack pointers, and
  user page mapping requests.
- `UserVirtualRange` for non-empty validated user pointer ranges.
- `UserReadableRange` and `UserWritableRange` for syscall copy direction before
  page-table permission checks.
- `UserCString` for readable syscall string candidates before NUL validation.
- `DmaPhysicalAddress` for physical addresses that may be programmed into
  device descriptors. This now exists in `kernel::memory::address`.
- `StorageDataAddress` for the active DMA data buffer passed through generic
  storage parsing. This now exists in `kernel::memory::address`.

The next implementation steps should focus on storage abstraction boundaries,
framebuffer/MMIO range wrappers, and internal page-table helper arithmetic. They
should avoid broad mechanical renames until the remaining high-risk boundaries
have typed constructors and callers.

## Migration Order

1. Wrap frame allocator return values.
2. Wrap UEFI memory-map physical starts and contiguous physical frame ranges.
3. Wrap user virtual addresses used by ELF loading and user stack setup.
4. Wrap syscall user pointer arguments after syscall dispatch converts from the
   raw ABI.
5. Wrap AHCI DMA physical addresses and keep register splitting at the hardware
   boundary.
6. Wrap MMIO and framebuffer physical ranges separately from allocatable RAM.
7. Replace internal raw address arithmetic after the boundary wrappers exist.

## Remaining Migration Order

1. Introduce framebuffer physical range and kernel virtual pointer wrappers for
   `paging::init`, `map_framebuffer`, and `main.rs` backbuffer setup.
2. Move internal paging helpers from raw `u64` arithmetic to local `PhysAddr` /
   `VirtAddr` arithmetic where this improves boundary clarity.
3. Keep ELF parser fields raw at the file-format layer, but convert loadable
   segment virtual addresses to typed user virtual ranges before mapping.
