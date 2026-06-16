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
- `PhysicalFrameStart`, `FrameCount`, and `PhysicalFrameRange` represent
  allocatable 4 KiB frame starts, non-zero frame counts, and contiguous frame
  ownership.
- `DmaPhysicalAddress` represents physical addresses that may be programmed
  into AHCI command headers, received-FIS buffers, command tables, and PRDT
  entries.
- `UserVirtualAddress` and `UserVirtualRange` represent non-null user virtual
  addresses and byte ranges before syscall copy validation.
- `UserReadableRange`, `UserWritableRange`, and `UserCString` represent syscall
  copy direction and string policy before `copy_from_user`, `copy_to_user`, and
  `copy_cstr_from_user`.
- User data page-table permission probes accept `UserReadableRange` and
  `UserWritableRange` before final raw slice creation. Raw `usize` pointers are
  limited to the final kernel slice/read boundary and diagnostic ABI inputs.
- `UserReadRequest` stores pending `read` destinations as `UserWritableRange`
  after syscall ABI pointer classification, so scheduler wait state does not
  retain a raw user pointer.
- Blocking `waitpid` stores deferred status-write destinations as
  `UserWritableRange` after syscall ABI pointer classification, so scheduler
  wait completions do not retain raw user pointers.
- `UserHeapBreakRequest` represents `brk` requests after syscall ABI address
  classification, so scheduler and heap code do not receive raw break
  addresses.
- `UserMappingUnmapRequest` represents `munmap` requests after syscall ABI
  address classification, so scheduler and mapping code do not receive raw
  unmap start addresses.
- `KernelStackGuardFault` stores guard, writable, and top addresses as
  `VirtAddr` after page-fault ABI values are classified at the kernel interrupt
  boundary.
- User task kernel stack tops are kept as `VirtAddr` across scheduler handoff
  paths and lower to raw `u64` only at the registered architecture installer
  and `SYSCALL` entry stack-top atomic boundary.
- Scheduler task snapshots retain the last resume address-space root as
  `PhysicalFrameStart` and the last resume kernel stack top as `VirtAddr`;
  raw numeric values are produced only by console and smoke formatting
  accessors.
- User virtual-memory task snapshots retain the `brk` heap base, current break,
  and next private mapping search start as `UserVirtualAddress`; raw numeric
  values are produced only by console and smoke formatting accessors.
- `user_stack::allocate_and_map_user_page(...) -> PhysicalFrameStart` now
  returns a typed physical frame start instead of a raw physical `u64`.
- `user_stack::map_user_range(...)` now accepts `UserVirtualAddress` and
  `PhysicalFrameStart` internally instead of crossing virtual and physical
  domains with raw `u64` parameters.
- `paging::map_kernel_mmio_range(...)` now accepts `PhysAddr` for the MMIO
  physical base address and returns `PageCount` for the mapped page coverage.
- PCI AHCI discovery stores BAR5 as `PhysAddr` and keeps that type through
  AHCI controller initialization and HBA MMIO mapping.
- `PhysicalFrameAllocator::add_region(...)` and `reserve_region*` accept
  `PhysAddr` physical starts and `FrameCount` frame counts before normalizing
  frame ranges.
- `AhciDmaBuffers` stores `DmaPhysicalAddress` fields internally, and
  `dma::split_address(...)` accepts `DmaPhysicalAddress`.
- `StorageDataAddress` represents the active DMA data buffer used by
  `BlockDevice`, AHCI service helpers, GPT parsing, and FAT32 parsing.
- `FramebufferPhysicalRange` represents the active graphics-mode framebuffer
  range passed from boot setup into paging.
- `KernelVirtualAddress` represents identity-mapped kernel virtual addresses
  such as the framebuffer backbuffer before display initialization converts it
  to a raw pointer.
- `PageCount` represents non-zero 4 KiB page counts before callers reserve
  virtual ranges, allocate user stacks, track private user mappings, or map
  paging helper byte ranges.
- `KernelVirtualRange` represents reserved higher-half kernel virtual ranges
  for future dynamic mappings without implying that pages are already mapped.
- `KernelVirtualRangeAllocator::new(...)` and `allocate_pages(...)` accept
  `PageCount` before reserving higher-half kernel virtual ranges.
- `process::UserProgramSpawnRequest::new(...)` and
  `user_stack::allocate_user_stack(...)` accept `PageCount` before mapping
  user stack pages.
- `UserMappings` stores private mapping record counts as `PageCount`;
  `map_private(...)` returns typed page counts, and `unmap_range(...)` accepts
  a typed unmap request before returning typed page counts for scheduler
  diagnostics. Automatic placement search cursors are kept as `UserPageStart`
  values before allocation diagnostics lower them for display. Split record
  starts created by `munmap` or fixed replacement are also passed as
  `UserPageStart` values before record updates.
- `task::UserMappingRequest` stores the requested `mmap` address only as
  `UserMappingPlacement`. Scheduler diagnostics derive the displayed requested
  address from that typed placement instead of retaining a raw syscall address.
- ELF entry points are converted to `UserVirtualAddress` immediately after
  header validation. Loader metadata and entry-segment membership checks use
  that typed value instead of reusing the raw ELF header field.
- ELF heap starts are accumulated as `UserPageStart` values after each load
  segment end is aligned to a user page. `LoadedElf` exposes the final heap
  start as `UserVirtualAddress`.
- `UserAddressSpace` represents a task-owned user PML4 root and is passed to
  ELF and user stack mapping helpers instead of relying on the active CR3.
- `paging::map_kernel_writable_no_execute_range(...)` is the boundary that
  turns a reserved `KernelVirtualRange` plus owned `PhysicalFrameRange` into
  mapped kernel-only writable non-executable pages.
- `paging::unmap_kernel_range_and_free_frames(...)` is the boundary that
  removes kernel virtual mappings and returns their backing physical frames
  through owner-checked allocator release.

## Remaining Raw Address API Inventory

The following APIs currently expose raw physical or virtual addresses across
module boundaries and should be typed before reusable frame allocation,
per-process page tables, or dynamic kernel mappings become general-purpose.

### Boot And Composition Root

- `src/main.rs`
  - `arch::init(kernel::interrupt::syscall_entry as *const () as u64)` passes a
    function address as a raw architecture argument.
  - `run_user_smoke_demo(...)` keeps user entry, user stack, `argv`, and `envp`
    addresses typed until `UserTaskContext` lowers them into the private
    `repr(C)` assembly ABI layout.

### Frame Allocation And Heap

- UEFI memory-map descriptors still expose raw physical starts at the firmware
  ABI boundary. The boot composition root wraps those starts as `PhysAddr`
  before registering regions with the frame allocator.
- `PhysicalFrameAllocator::allocate_frame() -> Option<PhysicalFrameStart>` returns
  a typed 4 KiB-aligned physical frame start.
- `PhysicalFrameAllocator::allocate_frames(FrameCount) ->
  Option<PhysicalFrameRange>` returns the typed physical start and frame count
  of a contiguous frame range.
- `kernel::memory::heap::init(heap_range: PhysicalFrameRange)` accepts a typed
  physical frame range that is also used as a virtual range while identity
  mapping is active.

### Paging

- Internal page-table helpers use local `PhysAddr` / `VirtAddr` arithmetic for
  page alignment, range ends, and page walks before converting to `x86_64`
  address types at mapper boundaries.
- `KernelVirtualRangeAllocator` accepts `PageCount` for managed virtual range
  sizing and individual higher-half virtual reservations.
- UEFI memory-map descriptors still expose raw physical starts because they are
  firmware ABI records; paging wraps those starts before internal identity-map
  calculations.

### User Memory

- `kernel::memory::user_stack::allocate_user_stack(address_space, ..., pages)
  -> AllocatedUserStack` accepts `PageCount`, maps into a specific user address
  space, and returns a typed user stack range with base, top, physical backing
  frames, and page count.
- `kernel::memory::user_mapping::UserMappings` converts syscall byte lengths
  into `PageCount` after ABI validation, then uses typed page counts for mapping
  records, successful allocations, typed unmap requests, and unmap results.
  Its automatic placement cursor remains a `UserPageStart` so the next private
  mapping search cannot retain an unaligned raw virtual address.
  When an unmap or fixed replacement splits a record, the right-side record
  start is classified as `UserPageStart` before `UserMappings` mutates the
  record table.
- The scheduler-owned `mmap` request keeps fixed requested addresses as
  `UserPageStart` inside `UserMappingPlacement`; the syscall raw requested
  address is used only to choose that placement or reject the request.
- `kernel::memory::user_heap::UserHeap` accepts `UserHeapBreakRequest` after
  `sys_brk` classifies the raw ABI value as either a current-break query or a
  validated user virtual address.
- `PreparedUserStack` exposes typed user virtual `stack_pointer`,
  `argument_values_pointer`, and `environment_values_pointer`.
- Initial user stack argument layout uses a local `UserVirtualAddress` cursor;
  raw writes are limited to copying bytes and pointer values into already
  reserved stack slots.
- Kernel stack guard-fault lookup accepts `VirtAddr` after `kernel::interrupt`
  classifies the raw architecture page-fault address. Diagnostic formatting
  lowers those typed virtual addresses back to raw numbers only at log output.
- Scheduler user-entry and timer-resume handoffs keep the selected user task
  kernel stack top as `VirtAddr`; architecture provider calls and the `SYSCALL`
  entry stack-top atomic are the remaining raw lowering points.
- Scheduler task snapshots keep the last resume address-space root as
  `PhysicalFrameStart` and the last resume kernel stack top as `VirtAddr`.
  Console and serial smoke diagnostics lower those values to raw numbers only
  when formatting diagnostic output.
- User virtual-memory task snapshots keep heap base, heap break, and private
  mapping next-start addresses as `UserVirtualAddress`. Console and serial
  smoke diagnostics lower those values to raw numbers only when formatting
  diagnostic output.
- `task::UserEntryArguments` is constructed from typed user pointers, and
  `UserTaskContext` keeps its raw `u64` register layout private to the
  `repr(C)` architecture entry ABI.
- `kernel::memory::user_pointer::copy_from_user` accepts
  `UserReadableRange`, and `copy_to_user` accepts `UserWritableRange`; syscall
  helpers convert raw ABI arguments first.
- Pending keyboard-backed `read` waits retain the validated destination as
  `UserWritableRange` until the task address space is active again, then
  revalidate page-table permissions before copying bytes.
- Blocking `waitpid` waits retain the optional status destination as
  `UserWritableRange` until the parent task address space is active again,
  then revalidate page-table permissions before writing the wait status.
- `kernel::memory::user_pointer::copy_cstr_from_user` accepts `UserCString`,
  which wraps a readable range capped by the syscall path-length policy.
- User data permission checks in `paging` and per-process `UserAddressSpace`
  consume `UserReadableRange` or `UserWritableRange`; they no longer accept raw
  pointer/length pairs after syscall pointer classification has succeeded.

### ELF Loading

- `kernel::elf::LoadedElf::entry_point() -> UserVirtualAddress` exposes a typed
  user virtual entry point.
- `kernel::elf::load_user_program(...)` and metadata validation keep the entry
  point as `UserVirtualAddress` after header validation; the raw ELF header
  field is not retained across segment membership checks.
- The ELF loader keeps the maximum page-aligned segment end as `UserPageStart`
  while deriving `LoadedElf::heap_start()`.
- `kernel::elf::load_user_program(address_space, ...)` maps loadable segments
  into the supplied user address-space root.
- `ProgramHeader::virtual_address() -> u64` remains raw because it exposes a
  field parsed directly from the ELF file. Loader validation converts accepted
  loadable segments to a local typed `UserVirtualRange` before page mapping and
  file-backed copying.
- ELF load-segment file-backed payload ranges remain `UserVirtualRange` values
  before page-copy calculations. Raw offsets are local to checked file/page
  overlap arithmetic.

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
- `FrameCount` for non-zero physical frame counts passed into frame allocator
  APIs.
- `PhysicalFrameRange` for frame start plus frame count. This is now the return
  type for contiguous physical frame allocations.
- `FramebufferPhysicalRange` for the active graphics-mode framebuffer physical
  range. This now exists in `kernel::memory::address`.
- `KernelVirtualAddress` for mapped kernel virtual addresses. This now exists
  in `kernel::memory::address`.
- `PageCount` for non-zero 4 KiB page counts passed through kernel virtual
  range allocator, user stack, user mapping, and paging helper APIs.
- `KernelVirtualRange` for non-empty page-aligned higher-half virtual ranges
  reserved by the kernel dynamic mapping allocator. This now exists in
  `kernel::memory::address`.
- `UserAddressSpace` for user page-table roots. This now exists in
  `kernel::memory::address_space`.
- `UserVirtualAddress` for non-null user pointers and ELF virtual addresses.
  This now covers loaded ELF entry points, prepared user stack pointers, and
  user page mapping requests.
- `UserVirtualRange` for non-empty validated user pointer ranges.
- `UserReadableRange` and `UserWritableRange` for syscall copy direction before
  page-table permission checks.
- `UserReadRequest` for scheduler-retained pending `read` destinations after
  raw syscall pointer classification.
- `UserWritableRange` for scheduler-retained blocking `waitpid` status
  destinations after raw syscall pointer classification.
- `UserCString` for readable syscall string candidates before NUL validation.
- `UserMappingUnmapRequest` for private `munmap` requests after syscall ABI
  classification.
- `VirtAddr` for scheduler-owned user task kernel stack top handoffs before
  architecture and `SYSCALL` entry boundaries.
- `PhysicalFrameStart` and `VirtAddr` for scheduler resume handoff diagnostic
  snapshots before console or smoke output formatting.
- `UserVirtualAddress` for user virtual-memory scheduler snapshots before
  console or smoke output formatting.
- `DmaPhysicalAddress` for physical addresses that may be programmed into
  device descriptors. This now exists in `kernel::memory::address`.
- `StorageDataAddress` for the active DMA data buffer passed through generic
  storage parsing. This now exists in `kernel::memory::address`.

The next implementation steps should focus on the remaining architecture ABI
boundaries. They should avoid broad mechanical renames until the remaining
high-risk boundaries have typed constructors and callers.

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

1. Keep architecture ABI fields raw at the assembly boundary, but keep their
   constructors and callers typed.
