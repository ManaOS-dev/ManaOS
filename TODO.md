# ManaOS TODO

This roadmap intentionally lists only unfinished work. Completed historical
items have been removed so the file stays useful for deciding the next task.

## Immediate Priorities

- [x] Set `NO_EXECUTE` on non-executable kernel and user mappings where appropriate.
- [x] Avoid parsing font data on every `draw_text` call; cache parsed font faces.
- [x] Replace the display command queue with a design that is correct for multi-producer use.
- [x] Replace cursor backup dimensions with the cursor size constant.
- [x] Split kernel console command dispatch into command-focused modules once more commands are added.

## Phase 5: Filesystem And Storage

### Storage Driver

- [x] Turn the AHCI probe path into a persistent block-device service instead of a boot-only smoke test.
- [x] Add a storage device registry with stable device identifiers.
- [x] Support multi-sector reads in the AHCI command path.
- [x] Support reads that cross FAT32 cluster boundaries.
- [x] Add AHCI error propagation instead of returning only `bool`.
- [x] Add AHCI interrupt-driven completion as an alternative to polling.
- [x] Add cache invalidation or explicit ownership rules for DMA buffers.
- [x] Add write support for AHCI sectors after read-only storage is stable.
- [x] Add a QEMU storage test mode that boots and verifies expected serial log lines automatically.

### Partition And Filesystem Parsing

- [x] Fall back to the backup GPT header when the primary header is invalid.
- [x] Support selecting a partition by type GUID or name instead of always selecting the first entry.
- [x] Validate FAT32 backup boot sector.
- [x] Implement FAT32 long file name entries.
- [x] Implement FAT32 directory traversal beyond the root directory.
- [x] Implement FAT32 file reads across full cluster chains.
- [x] Detect FAT32 cluster chain loops and invalid cluster numbers.
- [x] Implement FAT32 read-only directory listing API.
- [x] Add FAT32 write planning before mutating disk images.

### Virtual Filesystem

- [x] Add a real mount table with mount points and filesystem backends.
- [x] Mount FAT32 as a filesystem backend instead of copying one boot-time file into memory.
- [x] Add path traversal for directories and nested files.
- [x] Add file metadata operations such as `stat`.
- [x] Add `seek` support to file descriptors.
- [x] Add directory handles and `readdir` support.
- [x] Add read-only and writable mount flags.
- [x] Return richer filesystem errors and map them consistently to syscall errno values.
- [x] Add `/dev` directory listing.
- [x] Decide and document pathname normalization rules for `..`, repeated slashes, and trailing slashes.

### Kernel Console Commands

- [x] Split command parsing and individual commands out of `kernel::console::mod.rs`.
- [x] Add `ls`.
- [x] Add `pwd`.
- [x] Add `cd`.
- [x] Add `stat`.
- [x] Add `mounts`.
- [x] Add `hexdump`.
- [x] Add `grep`.
- [x] Add single-pipe command execution with `command | command`.
- [x] Add command history.
- [x] Add cursor movement and line editing.
- [x] Add scrollback for console output.
- [x] Make `cat /disk/hello.txt` a manual smoke test in the docs.

## Phase 6: Userland

### ELF And Process Loading

- [x] Implement a 64-bit ELF loader.
- [x] Validate ELF headers, program headers, and segment permissions.
- [x] Map user text, rodata, data, bss, stack, and guard pages with correct flags.
- [x] Pass `argc`, `argv`, and environment pointers to user entry points.
- [x] Load user programs from the filesystem instead of `include_bytes!`.
- [ ] Add `execve`.
- [ ] Add process identifiers and parent-child relationships.
- [ ] Add `wait` or `waitpid`.
- [ ] Add a minimal user shell process.
- [x] Add a userland test program that opens `/disk/hello.txt`.

### Syscall Surface

- [x] Define syscall numbers and ABI in a shared generated or copied contract for kernel and userland.
- [x] Add `lseek`.
- [x] Add `stat` or `newfstatat`.
- [x] Add `getdents64`.
- [ ] Add `brk` or another first heap-growth syscall.
- [ ] Add `mmap` and `munmap` planning.
- [ ] Add `nanosleep` or a minimal sleep syscall.
- [ ] Add `fork` or document why the first process model uses `spawn`/`exec` instead.
- [ ] Add syscall tracing controls.

### Userland Runtime

- [x] Grow the no-std userland support crate into a small runtime.
- [x] Add panic handling that exits with a clear status.
- [ ] Add basic formatting helpers for userland output.
- [x] Add file descriptor wrappers in userland.
- [x] Add argument parsing helpers.
- [x] Add fixed-buffer userland command modules with single-pipe execution.
- [x] Add build scripts for multiple userland binaries.
- [x] Add a userland smoke-test runner.

## Phase 7: Kernel Hardening

### Memory Management

- [x] Audit `PhysicalFrameAllocator` call sites and document reusable ownership invariants.
- [x] Replace the bump frame allocator with a reusable physical frame allocator.
- [x] Track reserved, used, and free physical frame ranges.
- [x] Design ownership rules for free, used, and reserved physical frame ranges.
- [x] Add a kernel virtual memory allocator for dynamic mappings, including writable NX mapping and generic unmap/free for kernel ranges.
- [ ] Add guard pages for kernel stacks; scheduler-owned task stacks now have mapped writable pages, unmapped virtual guards, and guard-fault diagnostics. Bootstrap/IST stacks remain.
- [x] Design kernel stack guard page placement and fault diagnostics.
- [x] Add per-process page tables; user smoke tasks now own separate address-space roots for ELF and stack mappings.
- [x] Reclaim finished user address spaces and return tracked user/page-table frames to the allocator.
- [x] Reclaim finished user task kernel stacks after `SYS_EXIT`.
- [x] Document the page ownership model required before per-process page tables.
- [x] Add copy-in/copy-out helpers with consistent user pointer validation.
- [x] Define a syscall-by-syscall user pointer validation policy.
- [x] Enforce writable, user, and executable page permissions in syscall validation.
- [x] Add boot-time self-checks for kernel and user mapping permissions.
- [x] Audit identity mapping lifetime and shrink it when possible.
- [x] Identify identity mappings that can be removed after boot-time hardware setup.
- [ ] Add typed physical and virtual address wrappers where raw `u64` still leaks across boundaries.
- [x] Inventory APIs where raw `u64` physical or virtual addresses cross module boundaries.
- [x] Add page fault diagnostics that include the current task and access type.

### Interrupts And Scheduling

- [ ] Parse ACPI RSDP and XSDT/RSDT.
- [ ] Parse ACPI MADT.
- [ ] Enable IOAPIC routing.
- [ ] Replace legacy PIC routing after IOAPIC is stable.
- [ ] Calibrate and use the Local APIC timer.
- [ ] Replace PIT scheduling ticks after Local APIC timer validation.
- [ ] Save and restore a full user trap frame on interrupt and syscall paths; one-shot user entry restores an initial full trap frame, SYSCALL entry captures runtime user frames on the task kernel stack, and the x86_64 PIT timer entry now captures, records, preempts, and resumes Ring 3 timer contexts. Broader multi-process lifecycle coverage remains.
- [x] Design the full user trap frame register layout.
- [x] Document the interrupt and syscall register sets that must be saved.
- [x] Make preemptive scheduling safe for the current one-shot user task path.
- [x] Add separate user stack slots so multiple user task records can coexist in the shared address space.
- [x] Prove timer preemption and resume across two user task records in storage smoke.
- [ ] Extend preemptive user scheduling across full process lifecycle paths now that user tasks own separate address spaces.
- [x] Allow multiple active user tasks to be scheduled by timer preemption in the current smoke lifecycle.
- [x] Move next active user task selection into the scheduler-owned lifecycle path.
- [x] Checklist the prerequisites for enabling user task preemption.
- [x] Add scheduler accounting and task state diagnostics.
- [x] Add a visible `tasks` console command for scheduler and user-preemption diagnostics.
- [x] Add a scheduler/preemption status strip to the console overlay.
- [x] Add user kernel stack reclaim accounting to scheduler diagnostics and the console overlay.
- [ ] Add kernel stack switching per task where needed; user task stacks are installed before entry and timer-context resume, Ring 3 timer interrupts use the installed TSS stack and save their raw frame there, and SYSCALL switches onto the task kernel stack. Bootstrap/IST stacks remain.
- [x] Design the per-task kernel stack switching policy.

### Context Switch And Task Refactoring

- [x] Separate kernel task context and user task context responsibilities.
- [x] Document the context switch ABI.
- [x] Verify the `UserTrapFrame` register layout against `context_switch.s` offsets.
- [x] Move user task exit and run-once lifecycle handling into a process lifecycle module.
- [x] Replace the global user-exit result latch with a scheduler-owned finished user exit queue.
- [x] Add explicit set/take invariants for the one-shot user exit return stack window.
- [x] Normalize user task scheduler state transitions.
- [x] Define the task metadata model needed before process identifiers and parent-child relationships.

### Synchronization And Concurrency

- [ ] Audit all interrupt-time locks for deadlock and priority inversion risk.
- [ ] Replace queues that have mismatched producer/consumer assumptions.
- [ ] Add explicit single-producer/single-consumer and multi-producer queue types.
- [ ] Define which APIs are callable from interrupt context.
- [ ] Add lock ordering notes for kernel subsystems.

## Phase 8: Drivers And Hardware

### Input

- [ ] Move keyboard layout choice behind a small configuration boundary.
- [ ] Add key release handling where useful.
- [ ] Add modifier state reporting for Shift, Control, Alt, and Super.
- [ ] Add Caps Lock state and LED updates.
- [ ] Add mouse wheel packet support.
- [ ] Add optional double-click and drag state at the UI layer, not the input driver layer.

### Display

- [ ] Add a text console with scrolling independent of the graphical overlay.
- [ ] Add damage tracking tests for dirty rectangles.
- [ ] Add primitive window/widget layer planning.
- [ ] Add bitmap image rendering support if the UI starts using assets.

### Future Hardware

- [ ] Investigate NVMe support after AHCI read/write is stable.
- [ ] Investigate USB keyboard and mouse support after ACPI/interrupt work.
- [ ] Add PCI capability parsing.
- [ ] Add MSI/MSI-X planning.

## Phase 9: Tooling, Tests, And Documentation

- [ ] Add a headless QEMU smoke test script for CI.
- [x] Add serial log assertions for boot milestones.
- [x] Add a disk-image fixture generator with multiple files and directories.
- [ ] Add parser unit tests for GPT and FAT32 using byte fixtures.
- [ ] Add syscall ABI tests for success and errno paths.
- [ ] Add userland build checks to CI for every committed user program.
- [x] Add architecture boundary checks that reject `arch` to `kernel` imports.
- [ ] Add docs for the direct maintainer branch workflow.
- [ ] Add docs for manual QEMU validation commands.
- [ ] Add a contributor-facing architecture map generated from the current module tree.
