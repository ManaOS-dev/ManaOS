# ManaOS Memory Management

This document records the invariants for ManaOS physical frame ownership,
reusable frame allocation, and dynamic kernel virtual mappings.

## Current `PhysicalFrameAllocator` Call Sites

`PhysicalFrameAllocator` is still the only physical frame source. It is passed from
the boot composition root into subsystems that need physical memory:

- `src/main.rs` registers UEFI conventional memory regions and passes the
  allocator through boot, storage, ELF loading, and user smoke setup.
- `src/kernel/boot/mod.rs` allocates the kernel heap after paging is enabled.
- `src/kernel/memory/paging.rs` allocates page-table frames and identity maps
  memory map ranges, framebuffer pages, and later MMIO pages.
- `src/kernel/memory/user_stack.rs` allocates user stack pages and user page
  table pages through `FrameAllocator` wrappers.
- `src/kernel/elf/loader.rs` allocates frames for user ELF `PT_LOAD` segments.
- `src/kernel/driver/storage/advanced_host_controller_interface/dma.rs`
  allocates AHCI command, FIS, command-table, and data DMA buffers.
- `src/kernel/driver/storage/advanced_host_controller_interface/controller.rs`
  and `host.rs` pass the allocator to controller and MMIO mapping setup.
- `src/kernel/driver/storage/mod.rs` passes the allocator into storage probing
  and persistent block-device setup.

## Allocator Invariants

The current physical frame allocator relies on these properties:

- Only UEFI `CONVENTIONAL` memory is registered before `ExitBootServices`.
- Memory registration APIs accept `PhysAddr` starts so reusable allocator
  callers cannot pass virtual addresses into the physical range model.
- Registered ranges are normalized to 4 KiB pages and skip physical address
  zero.
- Registered ranges are sorted and adjacent ranges are merged before
  allocation.
- Allocation scans tracked free ranges; a physical frame is returned at most
  once until its owner releases it back to the free pool.
- Deallocation requires the caller to provide the expected owner. Owner
  mismatches and double frees are rejected.
- Contiguous allocation is guaranteed only inside a single registered range.
- Returned physical addresses are assumed to be identity mapped when callers
  zero frames, build page tables, or hand addresses to AHCI DMA.
- Callers treat returned frames as exclusively owned until boot ends or until a
  later ownership model explicitly transfers them.

## Reusable Physical Frame Allocator Design

The allocator models physical memory as frame ranges with an explicit
state:

- `Reserved`: not allocatable. This includes physical address zero, firmware
  non-conventional memory, kernel image pages, boot modules, page tables,
  framebuffer and MMIO ranges, DMA buffers while device-owned, and guard pages.
- `Free`: allocatable conventional frames not currently owned by any subsystem.
- `Used`: frames owned by exactly one kernel subsystem, user address space, page
  table, heap, DMA buffer, or boot structure.

Required ownership rules:

- A frame may transition `Free -> Used` only through the allocator.
- A frame may transition `Used -> Free` only when its owner explicitly releases
  it and no page table, DMA descriptor, heap span, or task metadata still
  references it.
- A frame may transition `Free -> Reserved` for guard pages or hardware ranges.
- A frame may transition `Reserved -> Free` only for temporary boot-only
  reservations after the last user is proven gone.
- DMA frames must remain `Used` or `Reserved` while a device can read or write
  them.
- Page-table frames must remain `Used` until the owning address space is fully
  destroyed.
- User memory frames must remain `Used` until the future process lifecycle
  unmaps the owning address space and releases all mappings.

The allocator should expose single-frame and contiguous-frame allocation
without promising that separately allocated frames are adjacent. Contiguous
allocation should be used only for hardware or ABI requirements that actually
need physical contiguity.

## Page Ownership Model Before Per-Process Page Tables

Before per-process page tables are added, ownership is global and conservative:

- Kernel heap frames are kernel-owned for the lifetime of the kernel.
- Kernel page-table frames are kernel-owned and must never be freed while their
  page table can be active.
- User ELF segment frames are user-task-owned, but they live in the shared
  active address space today and must not be freed independently of the whole
  future process address space.
- User stack frames are user-task-owned; the guard page is reserved and must
  remain unmapped.
- Kernel stack frames are task-owned and mapped through higher-half kernel
  virtual ranges. The adjacent lower virtual guard page remains unmapped and
  does not consume a physical frame.
- AHCI DMA frames are storage-driver-owned and must not be reused while the
  controller can access descriptors or data buffers.
- Framebuffer and MMIO ranges are hardware-owned mappings. The allocator must
  not hand those physical frames out as regular RAM.
- Identity mappings are a mapping policy, not ownership. Unmapping an identity
  page is allowed only after all code paths that dereference the physical
  address as a virtual address have been removed or converted.

The first per-process page-table implementation must introduce an address-space
owner object before freeing user frames. Reclaiming user frames directly from
task exit is not safe until mappings, page tables, and task/process metadata
share one ownership boundary.

## Identity Mapping Audit Notes

Current code assumes identity mapping in these places:

- Page-table construction and CR3 table access in `paging.rs`.
- MMIO and framebuffer mapping setup in `paging.rs`.
- AHCI DMA buffer zeroing in `dma.rs`.
- User stack and user page-table mapping helpers in `user_stack.rs`.
- ELF segment loading when copying bytes into allocated physical frames.

The shrink path is therefore staged:

1. Keep identity mapping for page-table frames until a physical-memory window or
   recursive mapping exists.
2. Keep identity mapping for DMA buffers until storage code uses explicit kernel
   virtual mappings for buffer initialization.
3. Keep identity mapping for user frames until ELF loading and user stack setup
   write through explicit kernel mappings.
4. Convert framebuffer and MMIO users independently because hardware ranges are
   not regular frame allocator ownership.

## Kernel Virtual Range Reservation

The kernel now has a reusable allocator for reserved higher-half virtual
address ranges intended for dynamic mappings. It reserves virtual addresses;
page-table mapping, unmapping, and physical frame ownership remain separate
responsibilities.

This keeps the guard-page stack work incremental:

- reserve `N + 1` virtual pages for each guarded kernel stack,
- leave the lowest page unmapped as the guard page,
- map the remaining pages through `kernel::memory::paging` as kernel-only
  writable non-executable pages.

`kernel::task::stack` now stores that reservation metadata for schedulable
kernel and user tasks. The active scheduler-owned stack memory now uses the
reserved higher-half range: writable pages are backed by physical frames owned
as `KernelStack`, while the guard page stays unmapped.

Dynamic kernel mappings now have a generic unmap path:

- `paging::map_kernel_writable_no_execute_range(...)` maps an owned physical
  range into a reserved kernel virtual range.
- `paging::unmap_kernel_range_and_free_frames(...)` removes 4 KiB mappings and
  returns the backing frames only when the expected owner matches.
- `KernelVirtualRangeAllocator::free_pages(...)` releases virtual ranges for
  reuse after their mappings are gone.

Scheduler-owned stack physical frames and virtual ranges still live for the
lifetime of their task metadata. Freeing a task stack remains a process
lifecycle problem because no scheduler, interrupt, or architecture context may
retain a reference when the stack is destroyed.

## Replacement Checklist

- [x] Add frame-range state storage before adding `free`.
- [x] Import the boot memory map as `Reserved` and `Free` ranges explicitly.
- [x] Track explicit owners for page-table, heap, framebuffer backbuffer, DMA,
      kernel stack, user stack, and user ELF allocations.
- [x] Add owner classes and boot self-check coverage for kernel image, page
      tables, heap, kernel stack, framebuffer, MMIO, DMA, user stack, user ELF,
      and guard pages.
- [x] Mark runtime `LOADER_CODE` and UEFI MMIO reservations with precise
      kernel-image and MMIO owners during boot memory-map import.
- [ ] Mark future guard-page reservations with precise owners instead of
      relying on generic firmware reservations.
- [ ] Split `LOADER_DATA` reservations into narrower owners once boot pool
      allocations, font assets, and kernel image data have separate ranges.
- [x] Replace monotonic frame allocation with tracked free-range allocation and
      owner-checked frame release.
- [x] Add boot self-checks for released-frame reuse and owner-mismatch
      rejection.
- [x] Add boot self-checks for duplicate allocation, contiguous allocation
      boundaries, and reserved-range exclusion.
- [x] Add boot self-checks for zero-frame reservation and reserved/free/used
      range tracking.
- [x] Prove the owner-coverage allocator behavior change with
      `just storage-smoke`.
- [x] Add a boot self-check for dynamic kernel mapping map, unmap, virtual
      reuse, and physical reuse.
- [ ] Continue proving the boot path with `just storage-smoke` after every
      future allocator behavior change.
