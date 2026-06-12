# ManaOS Address Boundaries

This document inventories raw address APIs and defines where ManaOS should
introduce typed physical and virtual address wrappers.

## Address Type Boundary

ManaOS currently mixes raw `u64`, `usize`, pointers, and `x86_64` crate
`PhysAddr` / `VirtAddr` values. The migration should keep ABI-facing integers
at the edges and introduce typed addresses at kernel ownership boundaries:

- Syscall ABI arguments remain raw `u64` at `kernel::syscall::dispatch`, then
  convert to user pointer types or scalar values before validation.
- ELF file fields remain parsed as raw integer values, then convert to user
  virtual addresses before mapping.
- UEFI memory map physical starts remain raw at the boot boundary, then convert
  to physical frame/range types inside memory management.
- AHCI register programming may split physical addresses into low/high register
  halves only at the device register boundary.
- Kernel virtual pointers should be created only after a mapping helper proves
  the target range is mapped in the active address space.

## Raw Address API Inventory

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
  accepts a raw physical start address from the UEFI memory map.
- `BumpFrameAllocator::allocate_frame() -> Option<PhysicalFrameStart>` returns
  a typed 4 KiB-aligned physical frame start.
- `BumpFrameAllocator::allocate_frames(n) -> Option<PhysicalFrameRange>`
  returns the typed physical start and page count of a contiguous frame range.
- `kernel::memory::heap::init(heap_range: PhysicalFrameRange)` accepts a typed
  physical frame range that is also used as a virtual range while identity
  mapping is active.

### Paging And MMIO

- `kernel::memory::paging::init(..., framebuffer_base: u64, framebuffer_size:
  u64)` accepts a raw framebuffer physical range.
- `paging::map_kernel_mmio_range(..., physical_start: u64, size: u64)` accepts a
  raw physical MMIO range.
- Internal page-table helpers convert raw `u64` values into `PhysAddr` /
  `VirtAddr` locally; those conversions should move behind typed range
  wrappers.

### User Memory

- `kernel::memory::user_stack::allocate_user_stack(..., pages) ->
  UserVirtualAddress` returns a typed user virtual stack top.
- `PreparedUserStack` exposes typed user virtual `stack_pointer`,
  `argument_values_pointer`, and `environment_values_pointer`.
- `user_stack::allocate_and_map_user_page(virtual_address: UserVirtualAddress,
  flags) -> u64` accepts a typed user virtual address and returns a raw
  physical frame address.
- `user_stack::map_user_range(virtual_start, physical_start, pages, flags)`
  crosses both virtual and physical address domains with raw `u64`.
- `kernel::memory::user_pointer::copy_from_user` accepts
  `UserReadableRange`, and `copy_to_user` accepts `UserWritableRange`; syscall
  helpers convert raw ABI arguments first.

### ELF Loading

- `kernel::elf::LoadedElf::entry_point() -> UserVirtualAddress` exposes a typed
  user virtual entry point.
- `ProgramHeader::virtual_address() -> u64` exposes a raw user virtual segment
  address parsed from ELF.
- `loader::map_segment(...)` and related helpers pass raw physical frames and
  raw user virtual pages together.

### Storage And AHCI DMA

- `kernel::driver::storage::advanced_host_controller_interface::dma::AhciDmaBuffers`
  stores raw physical DMA buffer addresses.
- `dma::split_address(address: u64)` splits a raw physical address into AHCI
  register fields.
- `BlockDevice::read_logical_block(..., data_address: u64)` and related methods
  use raw physical DMA buffer addresses.
- FAT32 and GPT parsers accept `data_address: u64` and read through it as an
  identity-mapped pointer after storage fills the DMA buffer.

## Recommended Wrapper Types

Introduce wrappers in small steps:

- `PhysicalAddress` for physical byte addresses.
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
- `DmaPhysicalAddress` for physical addresses that may be programmed into
  device descriptors.

The first implementation should wrap constructor validation around alignment,
overflow, and address-space-range checks. It should avoid broad mechanical
renames until the highest-risk boundaries above have typed constructors and
callers.

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
