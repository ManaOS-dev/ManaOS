# ManaOS Completed TODO Archive

Completed items were moved out of [`TODO.md`](TODO.md) on 2026-06-14 so the
active roadmap can list only unfinished work.

## Phase 1: Process Lifecycle And User Program Execution

### `execve` And Image Replacement

- [x] Define the kernel-side `execve` contract for path, argv, envp, and file descriptor inheritance.
- [x] Add the shared syscall number and ABI constants for `execve`.
- [x] Add a no-std userland wrapper for `execve`.
- [x] Copy `argv` strings through the existing user pointer validation policy.
- [x] Copy `envp` strings through the existing user pointer validation policy.
- [x] Bound total argument and environment bytes with a documented limit.
- [x] Resolve executable paths through the current process filesystem namespace.
- [x] Reject directory targets with a stable errno value.
- [x] Reject non-ELF targets with a stable errno value.
- [x] Reuse the existing ELF validation path for `execve` images.
- [x] Add a failure smoke case for `execve` on a missing path.
- [x] Add a failure smoke case for `execve` on a directory path.
- [x] Add a failure smoke case for `execve` on a non-ELF file.
- [x] Document `execve` ownership and cleanup invariants in the process lifecycle docs.
- [x] Build and roll back an unpublished `execve` image candidate before returning unsupported.
- [x] Return a successful `execve` sentinel from syscall dispatch without writing an old-image return value.
- [x] Publish the prepared `execve` address space, stack, heap start, and trap frame through one scheduler-owned transition.
- [x] Replace the current user address space without leaking old user frames.
- [x] Preserve process ID across successful `execve`.
- [x] Preserve parent-child relationship across successful `execve`.
- [x] Reset user signal-like runtime state that should not survive image replacement.
- [x] Reset old `brk` heap bookkeeping during successful `execve`.
- [x] Reset old private `mmap` bookkeeping during successful `execve`.
- [x] Replace the user stack with a freshly built argv/envp stack image.
- [x] Reclaim the old user image only after the new image can no longer return to the old instruction pointer.
- [x] Add a no-return userland smoke path for successful self-`execve` without recursive re-exec.
- [x] Add current `execve` image generation, path, and old-image reclaim diagnostics to the `tasks` console command.
- [x] Add `execve` replacement-state diagnostics to the `tasks` console command once fallible candidate states exist.
- [x] Preserve open file descriptors that do not have close-on-exec semantics.
- [x] Add a boot smoke case that `execve`s a second user program from `/disk`.
- [x] Add close-on-exec metadata to file descriptors.
- [x] Close descriptors marked close-on-exec during successful `execve`.
- [x] Preserve current working directory across successful `execve`.

### `waitpid`, Exit Status, And Reaping

- [x] Define the `waitpid` syscall contract and supported option subset.
- [x] Add the shared syscall number and ABI constants for `waitpid`.
- [x] Add a no-std userland wrapper for `waitpid`.
- [x] Return `ECHILD` when the caller has no matching child.
- [x] Return `EINTR` only after an interrupt policy exists, or document that it is unsupported.
- [x] Add a negative smoke case for waiting on a non-child PID.
- [x] Document parent-child lifecycle state transitions.
- [x] Add scheduler-owned child exit records keyed by parent process ID.
- [x] Preserve exit status until the parent reaps the child.
- [x] Prevent double-reaping of the same child exit record.
- [x] Add serial assertions for the wait lifecycle smoke path.
- [x] Expose zombie and reaped counts in scheduler diagnostics.
- [x] Add `tasks` output for waiting, zombie, and reaped states.
- [x] Reap an already-exited child through scheduler-backed `waitpid`.
- [x] Support nonblocking wait with a minimal `WNOHANG` equivalent.
- [x] Add a userland smoke program that spawns a child and waits for exit.
- [x] Add a userland smoke program that verifies a nonzero child exit status.
- [x] Support blocking wait for any child.
- [x] Reparent orphaned children to a documented initial process policy.
- [x] Reclaim finished child address spaces after the exit record is safe.
- [x] Reclaim finished child kernel stacks after the exit record is safe.
- [x] Add scheduler assertions for impossible active, finished, and reclaiming transitions.
- [x] Document scheduler invariants for active, waiting, zombie, and reaped tasks.

### Spawn Model And Fork Decision

- [x] Decide whether the first stable process model is `spawn` plus `execve` or a minimal `fork`.
- [x] Document why copy-on-write `fork` is deferred if `spawn` is selected.
- [x] Define a kernel-internal spawn helper for creating a process from a filesystem path.
- [x] Add scheduler metadata for spawned process origin.
- [x] Add a smoke case for two concurrently spawned user programs.
- [x] Define how argv and envp are represented before user stack construction.
- [x] Add errno mappings for spawn path lookup failures.
- [x] Define how current working directory is inherited by spawned processes.
- [x] Add a no-std `getcwd` wrapper for task-owned current directory checks.
- [x] Add docs that compare the selected model with POSIX `fork` expectations.
- [x] Add TODO links from deferred `fork` work to the address-space copy plan.
- [x] Add errno mappings for spawn memory allocation failures.
- [x] Add a userland runtime helper for launching a child program.
- [x] Extend user-visible spawn beyond path-only launch with argv/envp vectors.
- [x] Define how inherited file descriptors are selected for spawned processes.
- [x] Add a smoke case for parent exit while child remains alive.
- [x] Move user file descriptor tables from global filesystem state to process-owned metadata before general spawn.
- [x] Enforce spawn descriptor inheritance selection with process-owned descriptor tables.

### Minimal User Shell

- [x] Add a minimal userland shell binary to the userland build.
- [x] Start the experimental userland shell after storage smoke gating.
- [x] Implement fixed-buffer command reading from stdin.
- [x] Implement whitespace tokenization without heap allocation.
- [x] Implement absolute path execution for user programs.
- [x] Implement relative path execution using the current working directory.
- [x] Implement `cd` through a user-visible syscall or runtime helper.
- [x] Implement `exit` with a configurable status code.
- [x] Implement `pwd` using the userland runtime path API.
- [x] Implement `help` with commands compiled into the shell.
- [x] Implement single-command execution without pipelines first.
- [x] Add fixed-buffer argv construction for command execution.
- [x] Add shell smoke logs for launching `file_demo`.
- [x] Add bounded error messages for command failures.
- [x] Add shell smoke logs for a missing command.
- [x] Keep the kernel console available while the user shell is experimental.
- [x] Run the experimental user shell stdin path through a command loop until EOF or `exit`.
- [x] Raise the userland image linker envelope for the experimental shell command loop.
- [x] Connect keyboard-backed stdin to the smoke-started userland shell standard input.
- [x] Keep the smoke-started userland shell alive while keyboard-backed stdin waits for input.
- [x] Document how to observe the experimental user shell entering and leaving in QEMU.

### User Process Scheduling

- [x] Add a storage smoke case with three active user processes.
- [x] Add diagnostics for last preemption reason per task.
- [x] Add diagnostics for last resume path per task.
- [x] Add a storage smoke case where one preempted process exits while another continues.
- [x] Extend timer preemption across general spawned user process lifecycles.
- [x] Save the full runtime user trap frame for every preempted user process.
- [x] Restore the full runtime user trap frame for resumed user processes.
- [x] Cover syscall return frames and timer interrupt frames with one scheduler path.
- [x] Verify that each user task resumes with its own address-space root.
- [x] Verify that each user task resumes with its own kernel stack.
- [x] Prevent scheduling a task while its address space is being reclaimed.

## Phase 2: Memory Safety, Address Spaces, And Stack Hardening

### Typed Address Boundaries

- [x] Make `PhysicalFrameStart` construction require `PhysAddr`.
- [x] Make `UserVirtualAddress` construction require `VirtAddr`.
- [x] Add `UserPageStart` for page-aligned user mapping APIs.
- [x] Add `FrameCount` for frame allocator APIs.
- [x] Keep physical frame range counts typed as `FrameCount`, with storage smoke coverage.
- [x] Add `PageCount` for kernel virtual range allocator APIs.
- [x] Keep dynamic kernel virtual range starts typed as `KernelPageStart`, with storage smoke coverage.
- [x] Keep dynamic kernel virtual range page counts typed as `PageCount`, with storage smoke coverage.
- [x] Add `PageCount` for user stack APIs.
- [x] Add `PageCount` for user mapping APIs.
- [x] Add `PageCount` for paging helper APIs.
- [x] Add a typed `brk` user heap request boundary and invalid ABI smoke coverage.
- [x] Add a typed `munmap` user mapping request boundary and storage smoke coverage.
- [x] Classify kernel stack guard-fault addresses as `VirtAddr` inside the task boundary.
- [x] Keep user task kernel stack-top handoffs as `VirtAddr` through the task architecture facade before architecture and `SYSCALL` entry raw boundaries.
- [x] Keep scheduler-owned kernel stack guard and writable starts typed as `KernelPageStart`, with storage smoke coverage.
- [x] Keep scheduler resume handoff diagnostic snapshots typed as `PhysicalFrameStart` and `VirtAddr` before console and smoke output formatting.
- [x] Keep user virtual-memory scheduler diagnostics snapshots typed as `UserVirtualAddress` before console and smoke output formatting.
- [x] Remove raw address accessors from scheduler and user virtual-memory task snapshots so console and smoke diagnostics lower typed task metadata only at formatting boundaries.
- [x] Keep pending user `read` destinations typed as `UserWritableRange` before scheduler wait-state retention.
- [x] Keep blocking `waitpid` status destinations typed as `UserWritableRange` before scheduler wait-completion retention.
- [x] Keep ELF load-segment file-backed payload ranges typed as `UserVirtualRange` before page-copy calculations.
- [x] Keep ELF load-segment memory and page ranges typed as `UserVirtualRange` and `UserPageStart` before mapping helpers consume them, with storage smoke coverage.
- [x] Keep private `mmap` syscall lengths typed as `UserMappingLength` before scheduler and mapping helpers consume them, with storage smoke coverage.
- [x] Keep user pointer page-table permission probes typed as `UserReadableRange` or `UserWritableRange` before raw slice creation.
- [x] Keep user pointer permission page walks typed as `UserPageStart` boundaries before page-table probes consume them, with storage smoke coverage.
- [x] Classify syscall copy pointer/length pairs through `UserReadableRange`, `UserWritableRange`, and `UserCString` constructors before lower copy helpers consume them.
- [x] Keep user address-space template self-check kernel probes typed as `VirtAddr` before memory APIs inspect shared kernel mappings.
- [x] Keep the saved kernel address-space root typed as `PhysicalFrameStart` before CR3 switching, with address-space template smoke coverage.
- [x] Keep user address-space permission self-check probe addresses typed as `VirtAddr` and `UserVirtualAddress`.
- [x] Keep scheduler-owned `mmap` requested addresses typed as `UserMappingPlacement` before diagnostics formatting.
- [x] Keep ELF entry points typed as `UserVirtualAddress` immediately after header validation.
- [x] Keep ELF heap starts typed as `UserPageStart` while accumulating load-segment ends.
- [x] Keep private `mmap` automatic placement search starts typed as `UserPageStart`.
- [x] Keep private mapping split record starts typed as `UserPageStart` during `munmap` and fixed replacement.
- [x] Keep private mapping record starts typed as `UserPageStart`.
- [x] Keep frame allocator tracked region starts typed as `PhysAddr` until numeric comparisons are required, with boot smoke coverage.
- [x] Keep AHCI DMA setup addresses typed as `DmaPhysicalAddress` until device-register splitting, with storage smoke coverage.
- [x] Keep user heap mapped-end helpers typed as `UserPageStart`, with `brk` smoke coverage.
- [x] Add checked `try_as_usize()` conversion helpers for typed addresses, with boot smoke coverage.
- [x] Keep MMIO identity-mapping page starts typed as `PhysicalFrameStart`, with APIC MMIO smoke coverage.
- [x] Keep kernel task stack-top context construction typed as `VirtAddr`, with kernel task stack smoke coverage.
- [x] Keep private mapping overlap and containment helpers typed as a page-aligned mapping range, with `mmap`/`munmap` smoke coverage.
- [x] Keep user trap-frame storage addresses typed as `VirtAddr` before scheduler metadata records them, with timer preemption smoke coverage.
- [x] Read shared timer interrupt frame storage, RIP, and RSP through typed wrappers before kernel timer diagnostics and scheduler trap-frame recording, with storage smoke coverage.
- [x] Classify returnable user-mode kernel return stack pointers as `VirtAddr` before private atomic storage, with boot smoke coverage.
- [x] Guard user entry and trap-frame register layouts with compile-time offset assertions before assembly-facing resumes.
- [x] Add storage smoke coverage for typed user entry argument pointer handoffs before first-entry context lowering.
- [x] Read user trap-frame RIP/RSP through typed `UserVirtualAddress` accessors before diagnostics and `execve` publication lower them for output.
- [x] Keep the `execve` published heap-start diagnostic boundary typed as `UserVirtualAddress` until serial formatting, with storage smoke coverage.
- [x] Pass the x86_64 syscall entry target through a typed `SyscallEntryAddress` before lowering it into the LSTAR MSR.
- [x] Classify the x86_64 timer interrupt entry stub through a typed `InterruptEntryAddress` before lowering it into the IDT gate.
- [x] Keep APIC MMIO physical bases typed as `ApicMmioAddress` before Local APIC, IOAPIC, and Local APIC timer register access lowers them to pointer-sized addresses.
- [x] Keep Local APIC timer calibration and active status snapshots typed as `ApicMmioAddress` before boot diagnostics lower them for serial output.
- [x] Carry page-fault diagnostics through a typed shared `PageFaultReport` before kernel diagnostics lower fault and instruction addresses for output.
- [x] Keep ACPI root-table, MADT, Local APIC, and IOAPIC physical address diagnostics typed as `PhysAddr` before serial output or APIC MMIO configuration.

### Address Space Lifecycle

- [x] Publish successful `execve` image replacement through one scheduler-owned transition.

## Phase 3: Synchronization, Interrupt Context, And Scheduler Robustness

### Scheduler State Machine

- [x] Add a scheduler invariant check that runs during storage smoke.
- [x] Add a named one-tick scheduler quantum policy with rationale comments and smoke diagnostics.

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
- [x] Add process identifiers and parent-child relationships.
- [x] Add a userland test program that opens `/disk/hello.txt`.

### Syscall Surface

- [x] Define syscall numbers and ABI in a shared generated or copied contract for kernel and userland.
- [x] Add `lseek`.
- [x] Add `stat` or `newfstatat`.
- [x] Add `getdents64`.
- [x] Add `brk` as the first heap-growth syscall.
- [x] Add an anonymous `mmap`/`munmap` syscall subset.
- [x] Support partial anonymous `munmap` with mapping-record splits.
- [x] Support non-overlapping fixed-address anonymous mappings.
- [x] Add file-backed private `mmap` for current VFS file descriptors.
- [x] Add replacement `MAP_FIXED` for private user mappings.
- [x] Add `nanosleep` or a minimal sleep syscall.
- [x] Add syscall tracing controls.

### Userland Runtime

- [x] Grow the no-std userland support crate into a small runtime.
- [x] Add panic handling that exits with a clear status.
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
- [x] Add visible frame allocator owner diagnostics to the `memory` console command and status strip.
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
- [x] Inventory APIs where raw `u64` physical or virtual addresses cross module boundaries.
- [x] Add page fault diagnostics that include the current task and access type.

### Interrupts And Scheduling

- [x] Parse ACPI RSDP and XSDT/RSDT.
- [x] Parse ACPI MADT.
- [x] Enable IOAPIC routing; APIC routing provider data now produces dry-run IOAPIC redirection entries, masked MMIO staging with readback diagnostics, Local APIC EOI-provider diagnostics, unified EOI dispatch, active route unmasking, and APIC EOI count diagnostics.
- [x] Replace legacy PIC routing after IOAPIC is stable; normal APIC boots now keep the legacy PIC masked and fallback-disabled, while the legacy PIC path remains for boots without APIC routing provider data.
- [x] Calibrate and use the Local APIC timer; boot now calibrates from a masked sample, masks the IOAPIC PIT timer route, and runs scheduler ticks from a periodic Local APIC timer.
- [x] Replace PIT scheduling ticks after Local APIC timer validation.
- [x] Add broader interrupt-controller diagnostics for Local APIC spurious vectors and unexpected external vectors.
- [x] Design the full user trap frame register layout.
- [x] Document the interrupt and syscall register sets that must be saved.
- [x] Make preemptive scheduling safe for the current one-shot user task path.
- [x] Add separate user stack slots so multiple user task records can coexist in the shared address space.
- [x] Prove timer preemption and resume across two user task records in storage smoke.
- [x] Allow multiple active user tasks to be scheduled by timer preemption in the current smoke lifecycle.
- [x] Move next active user task selection into the scheduler-owned lifecycle path.
- [x] Add a scheduler-owned active user lifecycle drain API for current smoke tasks.
- [x] Checklist the prerequisites for enabling user task preemption.
- [x] Add scheduler accounting and task state diagnostics.
- [x] Add a visible `tasks` console command for scheduler and user-preemption diagnostics.
- [x] Expose user virtual memory layout and per-task VM snapshots in `tasks`.
- [x] Add a scheduler/preemption status strip to the console overlay.
- [x] Add user kernel stack reclaim accounting to scheduler diagnostics and the console overlay.
- [x] Aggregate finished user task resource reclaim inside the scheduler lifecycle path.
- [x] Add per-task scheduler snapshots to the visible `tasks` console command.
- [x] Design the per-task kernel stack switching policy.

### Context Switch And Task Refactoring

- [x] Separate kernel task context and user task context responsibilities.
- [x] Document the context switch ABI.
- [x] Verify the `UserTrapFrame` register layout against `context_switch.s` offsets.
- [x] Move user task exit and run-once lifecycle handling into a process lifecycle module.
- [x] Replace the global user-exit result latch with a scheduler-owned finished user exit queue.
- [x] Add explicit set/take invariants for the one-shot user exit return stack window.
- [x] Close scheduler preemption from `SYS_EXIT` before returning through the one-shot exit stack.
- [x] Expose user-exit preemption window close accounting in scheduler diagnostics.
- [x] Replace boolean preemption diagnostics with explicit scheduler preemption states.
- [x] Normalize user task scheduler state transitions.
- [x] Define the task metadata model needed before process identifiers and parent-child relationships.

## Phase 9: Tooling, Tests, And Documentation

- [x] Add serial log assertions for boot milestones.
- [x] Add a disk-image fixture generator with multiple files and directories.
- [x] Add syscall ABI tests for success and errno paths.
- [x] Add architecture boundary checks that reject `arch` to `kernel` imports.
- [x] Add docs for the direct maintainer branch workflow.
- [x] Refresh manual QEMU validation docs with current commands and serial milestones.
- [x] Link manual QEMU validation docs from `README.md`.
- [x] Link manual QEMU validation docs from `CONTRIBUTING.md`.
- [x] Add troubleshooting notes for missing `OVMF.fd`.
- [x] Add troubleshooting notes for missing QEMU.
- [x] Add docs for reading serial output during manual QEMU runs.
- [x] Add docs for choosing between `just run` and `just storage-smoke`.
- [x] Keep root `Cargo.toml` dependency comments in English.
