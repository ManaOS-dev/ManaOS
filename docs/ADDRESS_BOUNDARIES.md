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
- Architecture entry-point function pointers should be classified before
  architecture initialization lowers them into IDT gates, CPU registers, or
  MSR writes.

## Implemented Address Boundaries

The following boundaries now use project-owned address wrappers instead of
untyped cross-domain `u64` values:

- `kernel::memory::address::PhysAddr` represents raw physical byte addresses.
- `kernel::memory::address::VirtAddr` represents raw virtual byte addresses
  for internal arithmetic that must not mix with physical addresses.
- Address wrappers expose checked `try_as_usize()` lowering before the final
  pointer or slice boundary, so call sites can choose an explicit error path
  instead of only panic-on-overflow `as_usize()` helpers.
- `PhysicalFrameStart`, `FrameCount`, and `PhysicalFrameRange` represent
  allocatable 4 KiB frame starts, non-zero frame counts, and contiguous frame
  ownership. `PhysicalFrameRange` exposes its count as `FrameCount` so callers
  lower to raw counts only at comparison or diagnostic boundaries.
- `DmaPhysicalAddress` represents physical addresses that may be programmed
  into AHCI command headers, received-FIS buffers, command tables, and PRDT
  entries.
- `UserVirtualAddress` and `UserVirtualRange` represent non-null user virtual
  addresses and byte ranges before syscall copy validation.
- `UserVirtualAddress::checked_sub()` keeps backward arithmetic in `VirtAddr`
  and revalidates the result before returning a non-null user address.
- `UserVirtualRange::end_exclusive()` keeps the exclusive range end as
  `VirtAddr`, so range consumers lower range ends only at comparison and
  page-table translation boundaries.
- `UserReadableRange`, `UserWritableRange`, and `UserCString` represent syscall
  copy direction and string policy before `copy_from_user`, `copy_to_user`, and
  `copy_cstr_from_user`.
- Syscall copy helpers classify raw pointer/length ABI pairs directly into
  `UserReadableRange`, `UserWritableRange`, or `UserCString` constructors before
  any page-table permission probe or string scan runs.
- User data page-table permission probes accept `UserReadableRange` and
  `UserWritableRange` before final raw slice creation. Raw `usize` pointers are
  limited to the final kernel slice/read boundary and diagnostic ABI inputs.
- User data page-table permission walks derive first and last user pages as
  `UserPageStart` values from `UserVirtualRange` before querying page-table
  flags.
- User address-space permission self-checks accept `VirtAddr` for kernel probe
  addresses and `UserVirtualAddress` for representative user addresses before
  forming copy-direction probe ranges.
- User address-space template self-checks accept the representative kernel
  probe address as `VirtAddr`, so the memory API does not receive a raw virtual
  pointer.
- The saved kernel address-space root remains raw only inside the private
  atomic storage boundary. Readers immediately classify it as
  `PhysicalFrameStart` before CR3 switching or address-space template smoke
  checks consume it.
- `UserReadRequest` stores pending `read` destinations as `UserWritableRange`
  after syscall ABI pointer classification, so scheduler wait state does not
  retain a raw user pointer.
- Blocking `waitpid` stores deferred status-write destinations as
  `UserWritableRange` after syscall ABI pointer classification, so scheduler
  wait completions do not retain raw user pointers.
- `UserHeapBreakRequest` represents `brk` requests after syscall ABI address
  classification, so scheduler and heap code do not receive raw break
  addresses.
- `UserHeap` keeps the page-aligned mapped extent as `UserPageStart` while
  growing and shrinking heap-backed mappings. The runtime mapped-end state
  therefore cannot retain an unaligned user virtual address.
  Break growth rounds requested user addresses through
  `UserVirtualAddress::align_up_to_page()` before heap code receives a
  `UserPageStart`.
- `UserMappingUnmapRequest` represents `munmap` requests after syscall ABI
  address classification, so scheduler and mapping code do not receive raw
  unmap start addresses. It retains the requested byte length as
  `UserMappingLength` before deriving page counts.
- `UserMappingLength` represents private `mmap` and `munmap` byte lengths after
  syscall ABI validation, so scheduler and mapping code do not receive raw
  length values when deriving page counts.
- `KernelPageStart` represents 4 KiB-aligned higher-half kernel virtual page
  starts used by dynamic kernel virtual ranges and scheduler-owned kernel stack
  guard and writable boundaries.
- `KernelStackGuardFault` stores guard and writable boundaries as
  `KernelPageStart` and the stack top as `VirtAddr` after page-fault ABI
  values are classified at the kernel interrupt boundary.
- `shared::PageFaultReport` carries the faulting virtual address, error bits,
  and instruction pointer from the architecture exception path through the
  registered reporter callback. The architecture layer classifies the raw CR2
  and exception-frame values before dispatch, and `kernel::interrupt`
  converts the virtual address fields into `VirtAddr` before diagnostics.
- `shared::TimerInterruptFrame` keeps the fixed timer interrupt ABI fields raw
  while exposing typed shared wrappers for the stack storage address,
  interrupted instruction pointer, and interrupted stack pointer. Kernel timer
  handling converts those wrappers into `VirtAddr` or `UserVirtualAddress`
  before recording scheduler metadata or serial diagnostics.
- `arch::x86_64::SyscallEntryAddress` represents the virtual entry target
  programmed into the `SYSCALL` LSTAR MSR. The composition root passes this
  typed value into architecture initialization, and raw numeric lowering stays
  inside the final MSR write boundary.
- `arch::x86_64::interrupt_descriptor_table` classifies the assembly timer
  interrupt entry target as an `InterruptEntryAddress` before lowering it into
  the IDT gate.
- User task kernel stack tops are kept as `VirtAddr` across scheduler handoff
  paths, through the task architecture facade, and through the registered
  architecture installer callback. The composition root converts the kernel
  `VirtAddr` into the x86_64-owned `PrivilegeStackTopAddress` before the final
  TSS write. The `SYSCALL` entry stack-top atomic remains a private raw storage
  boundary.
- The returnable user-mode entry stack pointer remains raw only at the
  assembly `set_user_return_stack` / `get_user_return_stack` ABI boundary and
  inside the private atomic storage slot. `kernel::task::process_lifecycle`
  classifies the value as `VirtAddr` before storing it or lowering it back to
  the ABI return type.
- Kernel task stack tops are passed into `TaskContext::from_stack(...)` as
  `VirtAddr` and lower to the private assembly-facing context layout only
  after the constructor has aligned the stack pointer.
- User trap-frame storage addresses are classified as `VirtAddr` before
  `kernel::task::record_current_user_trap_frame(...)`, so scheduler metadata
  does not receive a raw kernel stack address.
- `UserTrapFrame` keeps its `repr(C)` register fields raw for the architecture
  restore ABI, but kernel diagnostics and `execve` publication read user RIP
  and RSP through typed `UserVirtualAddress` accessors before formatting them.
- `execve` image publication keeps the replacement heap start as
  `UserVirtualAddress` until serial diagnostics need the numeric address.
- Scheduler task snapshots retain the last resume address-space root as
  `PhysicalFrameStart` and the last resume kernel stack top as `VirtAddr`;
  the snapshot API exposes typed accessors, and raw numeric values are produced
  only by console and smoke formatting code.
- User virtual-memory task snapshots retain the `brk` heap base, current break,
  and next private mapping search start as `UserVirtualAddress`; raw numeric
  values are produced only by console and smoke formatting code.
- `user_stack::allocate_and_map_user_page(...) -> PhysicalFrameStart` now
  returns a typed physical frame start instead of a raw physical `u64`.
- `user_stack::map_user_range(...)` now accepts `UserVirtualAddress` and
  `PhysicalFrameStart` internally instead of crossing virtual and physical
  domains with raw `u64` parameters.
- `paging::map_kernel_mmio_range(...)` now accepts `PhysAddr` for the MMIO
  physical base address and returns `PageCount` for the mapped page coverage.
  The identity-mapped page start is classified as `PhysicalFrameStart` before
  page-table mutation.
- PCI AHCI discovery stores BAR5 as `PhysAddr` and keeps that type through
  AHCI controller initialization and HBA MMIO mapping.
- APIC routing configuration stores Local APIC and IOAPIC MMIO physical bases
  as `ApicMmioAddress` before Local APIC, IOAPIC, and Local APIC timer
  register wrappers lower them to pointer-sized MMIO addresses.
- Local APIC timer calibration and active status snapshots retain the timer
  MMIO base as `ApicMmioAddress`. The private atomic slots remain raw only as
  publication boundaries, and boot diagnostics lower the typed value to `u64`
  only for serial output.
- `PhysicalFrameAllocator::add_region(...)` and `reserve_region*` accept
  `PhysAddr` physical starts and `FrameCount` frame counts before normalizing
  frame ranges.
- ACPI root-pointer, root-table, MADT, Local APIC, and IOAPIC diagnostics
  retain physical addresses as `PhysAddr` after firmware or ACPI byte fields
  are parsed. Boot diagnostics lower them only for serial output, and APIC
  routing setup lowers them only when constructing architecture-owned
  `ApicMmioAddress` values.
- `AhciDmaBuffers` stores `DmaPhysicalAddress` fields internally, and
  `dma::split_address(...)` accepts `DmaPhysicalAddress`. Storage smoke asserts
  that command-list, received-FIS, command-table, and data-buffer setup stays
  on typed DMA address boundaries before the final register split.
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
  The range start is stored as `KernelPageStart`, and the range length stays
  typed as `PageCount` until loop or diagnostic boundaries need a raw count.
- `KernelVirtualRangeAllocator::new(...)` accepts `KernelPageStart` and
  `PageCount`, and `allocate_pages(...)` accepts `PageCount` before reserving
  higher-half kernel virtual ranges.
- `process::UserProgramSpawnRequest::new(...)` and
  `user_stack::allocate_user_stack(...)` accept `PageCount` before mapping
  user stack pages.
- `UserMappings` stores private mapping record starts as `UserPageStart` and
  record counts as `PageCount`; `map_private(...)` returns typed page counts,
  and `unmap_range(...)` accepts a typed unmap request before returning typed
  page counts for scheduler diagnostics. Automatic placement search cursors
  are kept as `UserPageStart` values before allocation diagnostics lower them
  for display. Split record starts created by `munmap` or fixed replacement
  are also kept as `UserPageStart` values when the record table is updated.
  Internal overlap and containment helpers pass a private typed mapping range
  with `UserPageStart` start/end boundaries instead of raw start/end pairs.
- `task::UserMappingRequest` stores the requested `mmap` address only as
  `UserMappingPlacement`. Scheduler diagnostics derive the displayed requested
  address from that typed placement instead of retaining a raw syscall address.
- ELF entry points are converted to `UserVirtualAddress` immediately after
  header validation. Loader metadata and entry-segment membership checks use
  that typed value instead of reusing the raw ELF header field.
- ELF heap starts are accumulated as `UserPageStart` values after each load
  segment end is aligned to a user page. `LoadedElf` exposes the final heap
  start as `UserVirtualAddress`.
- ELF load-segment memory ranges are converted to `UserVirtualRange`, and their
  first/last mapped pages are converted to `UserPageStart` before mapping or
  page-copy helpers consume them. Storage smoke asserts the segment, page, and
  file-backed range markers.
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
  - The syscall entry function pointer is converted to
    `arch::x86_64::SyscallEntryAddress` before architecture initialization.
    The raw LSTAR value is produced only inside `init_syscall(...)`.
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
  of a contiguous frame range. Consumers read the count back as `FrameCount`
  before lowering it for heap-size checks or reclaim diagnostics.
- `kernel::memory::heap::init(heap_range: PhysicalFrameRange)` accepts a typed
  physical frame range that is also used as a virtual range while identity
  mapping is active.

### Paging

- Internal page-table helpers use local `PhysAddr` / `VirtAddr` arithmetic for
  page alignment, range ends, and page walks before converting to `x86_64`
  address types at mapper boundaries.
- `KernelVirtualRangeAllocator` accepts `KernelPageStart` for the managed
  virtual start and `PageCount` for managed virtual range sizing and
  individual higher-half virtual reservations. `KernelVirtualRange` preserves
  both values as typed accessors before page-table walkers lower the count for
  loops.
- UEFI memory-map descriptors still expose raw physical starts because they are
  firmware ABI records; paging wraps those starts before internal identity-map
  calculations.

### User Memory

- `kernel::memory::user_stack::allocate_user_stack(address_space, ..., pages)
  -> AllocatedUserStack` accepts `PageCount`, maps into a specific user address
  space, and returns a typed user stack range with base, top, physical backing
  frames, and page count.
- `kernel::memory::user_mapping::UserMappingLength` converts syscall byte
  lengths into `PageCount` after ABI validation. `UserMappings` then uses typed
  page counts for mapping records, successful allocations, typed unmap
  requests, and unmap results. `UserMappingUnmapRequest` stores the requested
  byte length as `UserMappingLength` before `UserMappings::unmap_range(...)`
  derives the removed page count.
  It keeps mapping record starts and the automatic placement cursor as
  `UserPageStart` so private mapping records and the next search position
  cannot retain unaligned raw virtual addresses.
  Its internal overlap and containment helpers pass a private typed mapping
  range with page-aligned start and exclusive-end boundaries before lowering
  addresses for comparisons.
  When an unmap or fixed replacement splits a record, the right-side record
  start stays classified as `UserPageStart` while `UserMappings` mutates the
  record table.
- The scheduler-owned `mmap` request keeps fixed requested addresses as
  `UserPageStart` inside `UserMappingPlacement`; the syscall raw requested
  address is used only to choose that placement or reject the request.
- The scheduler-owned `mmap` request keeps the requested byte length as
  `UserMappingLength`; raw syscall length values are used only to construct
  that typed request or reject the request.
- The scheduler-owned `munmap` request also keeps the requested byte length as
  `UserMappingLength`, so unmap page-count derivation consumes the same typed
  length wrapper as mapping allocation.
- `kernel::memory::user_heap::UserHeap` accepts `UserHeapBreakRequest` after
  `sys_brk` classifies the raw ABI value as either a current-break query or a
  validated user virtual address.
  Its growth and shrink helpers keep the aligned mapped-end boundary as
  `UserPageStart` before lowering it for comparisons or diagnostics. Growth
  obtains that boundary through `UserVirtualAddress::align_up_to_page()` instead
  of rounding raw integers inside the heap owner.
- `PreparedUserStack` exposes typed user virtual `stack_pointer`,
  `argument_values_pointer`, and `environment_values_pointer`.
- Initial user stack argument layout uses a local `UserVirtualAddress` cursor;
  raw writes are limited to copying bytes and pointer values into already
  reserved stack slots.
- Kernel stack guard-fault lookup accepts `VirtAddr` after `kernel::interrupt`
  receives a `shared::PageFaultReport` and classifies the page-fault virtual
  address. Scheduler-owned stack guard and writable starts stay classified as
  `KernelPageStart`; diagnostic formatting lowers those typed virtual
  addresses back to raw numbers only at log output.
- Scheduler user-entry and timer-resume handoffs keep the selected user task
  kernel stack top as `VirtAddr` through the task architecture facade and the
  registered architecture installer callback. The composition root adapts that
  value into the x86_64-owned `PrivilegeStackTopAddress`; the `SYSCALL` entry
  stack-top atomic is the remaining private raw lowering point.
- The returnable user-mode entry path receives the kernel return stack pointer
  from assembly as a raw ABI `usize`, immediately classifies it as `VirtAddr`,
  stores only the raw integer in a private atomic slot, and reclassifies the
  loaded value before returning it to the architecture stop path.
- Syscall and timer trap-frame storage addresses are raw only at the
  architecture/shared ABI capture point. The kernel interrupt and syscall
  bridges convert them to `VirtAddr` before the task scheduler records the
  captured `UserTrapFrame`.
- Timer interrupt frame RIP/RSP values are read through shared timer-frame
  wrappers, then classified as `UserVirtualAddress` before kernel diagnostics
  or scheduler-owned `UserTrapFrame` construction lower them again for the
  private resume ABI.
- User trap-frame RIP and RSP fields remain raw inside the fixed `repr(C)`
  resume frame. Kernel logging, diagnostics, and `execve` publication use
  typed `UserVirtualAddress` accessors before lowering those user addresses
  back to numbers for output.
- `execve` image publication also keeps the replacement heap start as
  `UserVirtualAddress` until serial diagnostics need the numeric address.
- Scheduler task snapshots keep the last resume address-space root as
  `PhysicalFrameStart` and the last resume kernel stack top as `VirtAddr`.
  Snapshot consumers use typed accessors, and console and serial smoke
  diagnostics lower those values to raw numbers only when formatting diagnostic
  output.
- User virtual-memory task snapshots keep heap base, heap break, and private
  mapping next-start addresses as `UserVirtualAddress`. Console and serial
  smoke diagnostics consume the typed accessors and lower those values to raw
  numbers only when formatting diagnostic output.
- `task::UserEntryArguments` is constructed from typed user pointers, and
  `UserTaskContext` keeps its raw `u64` register layout private to the
  `repr(C)` architecture entry ABI. Compile-time layout assertions guard the
  private layout, and storage smoke asserts the typed entry-argument handoff
  before diagnostics lower the pointers for serial output.
- `kernel::memory::user_pointer::copy_from_user` accepts
  `UserReadableRange`, and `copy_to_user` accepts `UserWritableRange`; syscall
  helpers convert raw ABI arguments first.
- `UserVirtualRange` derives permission-check page-walk boundaries as
  `UserPageStart` values, so active and per-process user page-table probes do
  not use untyped virtual addresses for user page starts.
- `UserVirtualRange::end_exclusive()` returns `VirtAddr`, so range-end
  arithmetic stays typed until last-page derivation, comparison, or page-table
  translation needs a raw value.
- Pending keyboard-backed `read` waits retain the validated destination as
  `UserWritableRange` until the task address space is active again, then
  revalidate page-table permissions before copying bytes.
- Blocking `waitpid` waits retain the optional status destination as
  `UserWritableRange` until the parent task address space is active again,
  then revalidate page-table permissions before writing the wait status.
- `kernel::memory::user_pointer::copy_cstr_from_user` accepts `UserCString`,
  which wraps a readable range capped by the syscall path-length policy.
- Syscall buffer helpers use `UserReadableRange`, `UserWritableRange`, and
  `UserCString` syscall constructors so raw pointer/length pairs do not leak
  past copy-direction classification.
- User data permission checks in `paging` and per-process `UserAddressSpace`
  consume `UserReadableRange` or `UserWritableRange`; they no longer accept raw
  pointer/length pairs after syscall pointer classification has succeeded.
- Per-process address-space permission self-checks keep the kernel probe as
  `VirtAddr` and user probes as `UserVirtualAddress`; raw `usize` lowering is
  limited to final diagnostics and kernel slice construction.
- User address-space template self-checks keep the representative kernel probe
  as `VirtAddr`; the boot smoke call site performs the architecture pointer
  lowering and checked numeric conversion before entering the memory API.
- The saved kernel address-space root is stored as a raw integer only because
  it crosses a private `AtomicU64` boundary. Kernel address-space switching and
  template smoke checks reload it as `PhysicalFrameStart` before passing it to
  page-table helpers.

### ELF Loading

- `kernel::elf::LoadedElf::entry_point() -> UserVirtualAddress` exposes a typed
  user virtual entry point.
- `kernel::elf::load_user_program(...)` and metadata validation keep the entry
  point as `UserVirtualAddress` after header validation; the raw ELF header
  field is not retained across segment membership checks.
- The ELF loader keeps the maximum page-aligned segment end as `UserPageStart`
  while deriving `LoadedElf::heap_start()`. Each validated segment exclusive
  end is classified as `UserVirtualAddress` and rounded through
  `UserVirtualAddress::align_up_to_page()` before heap-start accumulation.
- `kernel::elf::load_user_program(address_space, ...)` maps loadable segments
  into the supplied user address-space root.
- `ProgramHeader::virtual_address() -> u64` remains raw because it exposes a
  field parsed directly from the ELF file. Loader validation converts accepted
  loadable segments to a local typed `UserVirtualRange` before page mapping and
  file-backed copying.
- ELF load-segment file-backed payload ranges remain `UserVirtualRange` values
  before page-copy calculations. Raw offsets are local to checked file/page
  overlap arithmetic.
- ELF load-segment page walks receive `UserPageStart` boundaries from
  `LoadSegmentRange`; raw segment virtual addresses are not passed back into
  the mapping helper after validation.

### Storage And AHCI DMA

- The storage parser and block-device path now uses `StorageDataAddress`. Raw
  pointer conversion is limited to sector-slice creation after the block device
  fills the active DMA data buffer.
- AHCI DMA setup keeps command-list, received-FIS, command-table, and data
  buffer addresses as `DmaPhysicalAddress` until device registers need low/high
  halves.

## Recommended Wrapper Types

Continue introducing wrappers in small steps:

- `PhysAddr` for physical byte addresses. This now exists in
  `kernel::memory::address`.
- `PhysAddr` for ACPI table and interrupt-controller physical addresses before
  serial diagnostics or architecture-specific APIC MMIO wrappers consume them.
- `VirtAddr` for virtual byte addresses. This now exists in
  `kernel::memory::address`.
- `PhysicalFrameStart` for 4 KiB-aligned physical frame starts.
- `FrameCount` for non-zero physical frame counts passed into frame allocator
  APIs.
- `PhysicalFrameRange` for frame start plus `FrameCount`. This is now the
  return type for contiguous physical frame allocations.
- `FramebufferPhysicalRange` for the active graphics-mode framebuffer physical
  range. This now exists in `kernel::memory::address`.
- `KernelVirtualAddress` for mapped kernel virtual addresses. This now exists
  in `kernel::memory::address`.
- `PageCount` for non-zero 4 KiB page counts passed through kernel virtual
  range allocator, user stack, user mapping, and paging helper APIs.
- `KernelVirtualRange` for non-empty page-aligned higher-half virtual ranges
  reserved by the kernel dynamic mapping allocator. Its start is now
  `KernelPageStart`, and its page count is now exposed as `PageCount`.
- `KernelPageStart` for page-aligned higher-half virtual page starts such as
  dynamic kernel virtual range starts and scheduler-owned kernel stack guard
  and writable boundaries.
- `UserAddressSpace` for user page-table roots. This now exists in
  `kernel::memory::address_space`.
- `UserVirtualAddress` for non-null user pointers and ELF virtual addresses.
  This now covers loaded ELF entry points, prepared user stack pointers, and
  user page mapping requests.
- `UserVirtualAddress::checked_sub()` for backward user address arithmetic
  before syscall range helpers and stack-layout code observe the result.
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
- `UserMappingLength` for private `mmap` and `munmap` lengths after syscall ABI
  classification.
- `VirtAddr` for scheduler-owned user task kernel stack top handoffs through
  the task architecture facade before architecture and `SYSCALL` entry raw
  boundaries.
- `PhysicalFrameStart` and `VirtAddr` for scheduler resume handoff diagnostic
  snapshots before console or smoke output formatting.
- `UserVirtualAddress` for user virtual-memory scheduler snapshots before
  console or smoke output formatting.
- `DmaPhysicalAddress` for physical addresses that may be programmed into
  device descriptors. This now exists in `kernel::memory::address`.
- `StorageDataAddress` for the active DMA data buffer passed through generic
  storage parsing. This now exists in `kernel::memory::address`.
- `ApicMmioAddress` for APIC-family MMIO physical bases before architecture
  register access lowers them to pointer-sized addresses.
- `ApicMmioAddress` for Local APIC timer calibration and active status
  snapshots before boot diagnostics lower them for serial output.
- `SyscallEntryAddress` for the architecture-owned virtual entry point
  programmed into x86_64 `SYSCALL` LSTAR.
- `InterruptEntryAddress` for architecture-owned interrupt entry points
  programmed into x86_64 IDT gates.
- `PrivilegeStackTopAddress` for architecture-owned Ring 0 stack tops
  programmed into the x86_64 TSS before user-mode privilege transitions.
- `PageFaultReport`, `PageFaultAddress`, `PageFaultErrorBits`, and
  `PageFaultInstructionPointer` for the shared page-fault callback boundary
  before kernel diagnostics classify those virtual addresses as `VirtAddr`.
- `TimerInterruptFrame`, `TimerFrameStorageAddress`,
  `TimerFrameInstructionPointer`, and `TimerFrameStackPointer` for the shared
  timer interrupt callback boundary before kernel timer handling classifies
  those addresses as `VirtAddr` or `UserVirtualAddress`.

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
