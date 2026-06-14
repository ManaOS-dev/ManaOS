# ManaOS TODO

This roadmap intentionally lists only unfinished work. Completed historical
items live in [`TODO_COMPLETED.md`](TODO_COMPLETED.md).

Keep each checked-off item backed by a focused branch, a clear commit, and the
narrowest useful verification command. Split any item that grows beyond one
reviewable unit before implementing it.

## Phase 1: Process Lifecycle And User Program Execution

### `execve` And Image Replacement

- [ ] Add `execve` replacement-state diagnostics to the `tasks` console command once fallible candidate states exist.

### `waitpid`, Exit Status, And Reaping

- [ ] Support blocking wait for any child.
- [ ] Support nonblocking wait with a minimal `WNOHANG` equivalent.
- [ ] Reparent orphaned children to a documented initial process policy.
- [ ] Reclaim finished child address spaces after the exit record is safe.
- [ ] Reclaim finished child kernel stacks after the exit record is safe.
- [ ] Add a userland smoke program that spawns a child and waits for exit.
- [ ] Add a userland smoke program that verifies a nonzero child exit status.

### Spawn Model And Fork Decision

- [ ] Decide whether the first stable process model is `spawn` plus `execve` or a minimal `fork`.
- [ ] Document why copy-on-write `fork` is deferred if `spawn` is selected.
- [ ] Move user file descriptor tables from global filesystem state to process-owned metadata before general spawn.
- [ ] Define how inherited file descriptors are selected for spawned processes.
- [ ] Define how current working directory is inherited by spawned processes.
- [ ] Add a userland runtime helper for launching a child program.
- [ ] Add errno mappings for spawn memory allocation failures.
- [ ] Add a smoke case for parent exit while child remains alive.
- [ ] Add docs that compare the selected model with POSIX `fork` expectations.
- [ ] Add TODO links from deferred `fork` work to the address-space copy plan.

### Minimal User Shell

- [ ] Add a minimal userland shell binary to the userland build.
- [ ] Start the userland shell as the initial interactive process after smoke gating.
- [ ] Implement fixed-buffer command reading from stdin.
- [ ] Implement whitespace tokenization without heap allocation.
- [ ] Implement absolute path execution for user programs.
- [ ] Implement relative path execution using the current working directory.
- [ ] Implement `cd` through a user-visible syscall or runtime helper.
- [ ] Implement `pwd` using the userland runtime path API.
- [ ] Implement `exit` with a configurable status code.
- [ ] Implement `help` with commands compiled into the shell.
- [ ] Implement single-command execution without pipelines first.
- [ ] Add fixed-buffer argv construction for command execution.
- [ ] Add bounded error messages for command failures.
- [ ] Add shell smoke logs for launching `file_demo`.
- [ ] Add shell smoke logs for a missing command.
- [ ] Keep the kernel console available while the user shell is experimental.
- [ ] Document how to enter and leave the user shell in QEMU.

### User Process Scheduling

- [ ] Extend timer preemption across general spawned user process lifecycles.
- [ ] Save the full runtime user trap frame for every preempted user process.
- [ ] Restore the full runtime user trap frame for resumed user processes.
- [ ] Cover syscall return frames and timer interrupt frames with one scheduler path.
- [ ] Verify that each user task resumes with its own address-space root.
- [ ] Verify that each user task resumes with its own kernel stack.
- [ ] Prevent scheduling a task while its address space is being reclaimed.
- [ ] Add scheduler assertions for impossible active, finished, and reclaiming transitions.
- [ ] Document scheduler invariants for active, waiting, zombie, and reaped tasks.

## Phase 2: Memory Safety, Address Spaces, And Stack Hardening

### Typed Address Boundaries

- [ ] Replace raw physical address parameters with `PhysAddr` in remaining memory APIs.
- [ ] Replace raw virtual address parameters with `VirtAddr` in remaining memory APIs.
- [ ] Add typed page-aligned address wrappers where alignment is required by construction.
- [ ] Add typed page count wrappers for APIs that distinguish bytes from pages.
- [ ] Add typed frame count wrappers for frame allocator APIs.
- [ ] Replace raw `u64` address values in scheduler diagnostics snapshots.
- [ ] Replace raw `u64` address values in task metadata where feasible.
- [ ] Replace raw `u64` address values in storage DMA setup boundaries.
- [ ] Replace raw `u64` address values in ELF segment mapping boundaries.
- [ ] Replace raw `u64` address values in syscall memory helpers.
- [ ] Add conversion helpers with explicit checked failure modes.
- [ ] Add docs for when raw numeric addresses are still allowed.
- [ ] Add compile-time layout assertions where typed wrappers cross assembly-facing structs.
- [ ] Add tests or smoke assertions for representative typed address conversions.

### Kernel Stack Guards

- [ ] Finish guard pages for bootstrap kernel stacks.
- [ ] Finish guard pages for architecture-owned TSS stacks.
- [ ] Finish guard pages for IST stacks.
- [ ] Represent bootstrap stack ownership in memory diagnostics.
- [ ] Represent TSS stack ownership in memory diagnostics.
- [ ] Represent IST stack ownership in memory diagnostics.
- [ ] Add page fault diagnostics that identify a bootstrap stack guard hit.
- [ ] Add page fault diagnostics that identify a TSS stack guard hit.
- [ ] Add page fault diagnostics that identify an IST stack guard hit.
- [ ] Add a documented policy for which stacks may be shared during early boot.
- [ ] Add a documented policy for when bootstrap stacks stop being used.
- [ ] Add a boot-time self-check for kernel stack guard placement.
- [ ] Add a smoke assertion for scheduler-owned stack guard diagnostics.
- [ ] Document all kernel stack classes in `docs/KERNEL_STACKS.md`.

### Address Space Lifecycle

- [ ] Add explicit address-space state transitions for building, active, exiting, and reclaimed states.
- [ ] Convert `execve` candidate image construction to fallible allocation instead of panic-on-OOM.
- [ ] Ensure failed spawn cleanup returns all newly allocated frames.
- [ ] Track page-table frame ownership per process in diagnostics.
- [ ] Track mapped user frame ownership per process in diagnostics.
- [ ] Track guard page virtual reservations per process in diagnostics.
- [ ] Add per-process address-space reclaim counters.
- [ ] Add serial smoke assertions for address-space reclaim after process exit.
- [ ] Add serial smoke assertions for address-space reclaim after failed image load.
- [ ] Add documentation for address-space publication and rollback.
- [ ] Audit address-space APIs for init-order assumptions.

### User Mapping Policy

- [ ] Add named permission presets for user text, rodata, data, heap, stack, and MMIO-denied mappings.
- [ ] Audit every user mapping call site for writable and executable flag combinations.
- [ ] Add a policy test that rejects writable executable user mappings unless explicitly allowed.
- [ ] Add a policy test that rejects kernel-access-only pages from user pointer validation.
- [ ] Add page fault messages that report expected mapping class.
- [ ] Add `tasks` output that groups mappings by class.
- [ ] Add a userland fault smoke for executing non-executable data.
- [ ] Add a userland fault smoke for writing read-only rodata.
- [ ] Add a userland fault smoke for reading an unmapped guard page.
- [ ] Document mapping classes in `docs/MEMORY_MANAGEMENT.md`.

### Heap And `mmap`

- [ ] Add `mprotect` planning for changing user mapping permissions.
- [ ] Add `mremap` planning for growing and moving mappings.
- [ ] Add shared anonymous mapping planning, or document why it is deferred.
- [ ] Add file-backed mapping writeback planning, or document why private-only mappings remain.
- [ ] Add map-count limits per process.
- [ ] Add total mapped-byte limits per process.
- [ ] Add errno coverage for map-count exhaustion.
- [ ] Add errno coverage for address-space exhaustion.
- [ ] Add stress smoke for many small anonymous mappings.
- [ ] Add stress smoke for alternating partial `munmap` operations.
- [ ] Add userland runtime helpers for page size and mapping flags.
- [ ] Document `brk` and `mmap` interaction rules.

## Phase 3: Synchronization, Interrupt Context, And Scheduler Robustness

### Interrupt-Time Lock Audit

- [ ] Inventory every lock that can be reached from interrupt context.
- [ ] Inventory every lock that can be reached from exception context.
- [ ] Inventory every lock that can be reached from syscall context.
- [ ] Mark APIs that are interrupt-callable in module documentation.
- [ ] Mark APIs that must never be interrupt-callable in module documentation.
- [ ] Add lock ordering notes for memory, scheduler, console, storage, and input subsystems.
- [ ] Add assertions or diagnostics for lock acquisition in forbidden contexts where practical.
- [ ] Audit serial logging from interrupt and exception paths.
- [ ] Audit console rendering from interrupt and exception paths.
- [ ] Audit allocator calls from interrupt and exception paths.
- [ ] Audit storage completion paths for lock ordering.
- [ ] Audit scheduler tick paths for lock ordering.
- [ ] Document known interrupt-time lock risks before changing queue primitives.

### Queue Primitive Cleanup

- [ ] Define a single-producer single-consumer queue type.
- [ ] Define a multi-producer single-consumer queue type.
- [ ] Define a bounded interrupt-safe byte queue type.
- [ ] Replace input queues that assume a different producer count than they actually have.
- [ ] Replace display queues that assume a different producer count than they actually have.
- [ ] Add overflow counters for interrupt-fed queues.
- [ ] Add drop-policy documentation for input queues.
- [ ] Add backpressure documentation for non-interrupt queues.
- [ ] Add unit-level tests for queue wraparound behavior where host tests are possible.
- [ ] Add boot smoke diagnostics for queue overflow counters.
- [ ] Document which queue types may be used with interrupts disabled.

### Scheduler State Machine

- [ ] Define a single enum for user task runnable, waiting, zombie, and reclaimed states.
- [ ] Remove duplicated task state transition logic from scheduler submodules.
- [ ] Add transition helper functions with documented preconditions.
- [ ] Add transition diagnostics for invalid state changes.
- [ ] Add per-state task counters to scheduler snapshots.
- [ ] Add a scheduler invariant check that runs during storage smoke.
- [ ] Add a scheduler invariant check that can be triggered from the `tasks` command.
- [ ] Add timeout-aware waiting state for sleep and waitpid.
- [ ] Add wake reason diagnostics for sleeping tasks.
- [ ] Add wake reason diagnostics for waitpid tasks.
- [ ] Document state transitions in a scheduler lifecycle diagram.

### Syscall And Trap Robustness

- [ ] Audit syscall entry for all clobbered registers.
- [ ] Audit syscall return for user flags handling.
- [ ] Audit syscall return for canonical user instruction pointers.
- [ ] Audit syscall return for canonical user stack pointers.
- [ ] Add negative tests for invalid syscall numbers.
- [ ] Add negative tests for unsupported syscall flags.
- [ ] Add tracing controls per process instead of global-only tracing.
- [ ] Add syscall latency counters for slow path diagnostics.
- [ ] Add syscall failure counters grouped by errno.
- [ ] Add trap diagnostics that include process ID and thread-like task ID.
- [ ] Document syscall argument ownership rules for pointer and scalar arguments.

### Timer And Preemption Policy

- [ ] Add configurable scheduler quantum constants with rationale comments.
- [ ] Add diagnostics for missed timer ticks.
- [ ] Add diagnostics for timer ticks skipped while interrupts are masked.
- [ ] Add a preemption disable counter for critical scheduler sections.
- [ ] Add assertions for unbalanced preemption disable scopes.
- [ ] Add per-task runtime accounting in timer ticks.
- [ ] Add per-task preemption count accounting.
- [ ] Add fairness smoke for two CPU-bound user tasks.
- [ ] Add fairness smoke for one sleeping and one CPU-bound user task.
- [ ] Document the current single-core scheduling assumptions.

## Phase 4: Filesystem, Storage, And Device I/O Expansion

### Filesystem Tests And Semantics

- [ ] Add parser unit tests for GPT primary header parsing using byte fixtures.
- [ ] Add parser unit tests for GPT backup header parsing using byte fixtures.
- [ ] Add parser unit tests for GPT partition entry validation using byte fixtures.
- [ ] Add parser unit tests for FAT32 boot sector validation using byte fixtures.
- [ ] Add parser unit tests for FAT32 FSInfo validation using byte fixtures.
- [ ] Add parser unit tests for FAT32 long file name decoding using byte fixtures.
- [ ] Add parser unit tests for FAT32 cluster-chain loop detection using byte fixtures.
- [ ] Add parser unit tests for FAT32 invalid cluster rejection using byte fixtures.
- [ ] Add VFS tests for repeated slash normalization.
- [ ] Add VFS tests for trailing slash normalization.
- [ ] Add VFS tests for `..` traversal at mount roots.
- [ ] Add VFS tests for nested mount point traversal.
- [ ] Add VFS tests for read-only mount write rejection.
- [ ] Add VFS tests for directory read offsets.
- [ ] Add VFS tests for file descriptor seek edge cases.

### FAT32 Mutation Support

- [ ] Define the FAT32 write transaction boundary for one file write.
- [ ] Define FAT32 allocation rollback behavior on partial failure.
- [ ] Implement free cluster search with bounded scan diagnostics.
- [ ] Implement FAT entry updates for newly allocated cluster chains.
- [ ] Implement directory entry creation for short names.
- [ ] Implement directory entry creation for long file names.
- [ ] Implement file growth within an existing cluster chain.
- [ ] Implement file growth across newly allocated clusters.
- [ ] Implement file truncation with cluster release.
- [ ] Implement directory creation.
- [ ] Implement unlink for regular files.
- [ ] Implement empty-directory removal.
- [ ] Add write-through or flush semantics for modified FAT sectors.
- [ ] Add smoke tests that create, read, and delete a file on the disk image.
- [ ] Document the corruption model until journaling exists.

### Storage Reliability

- [ ] Add AHCI retry policy for transient command failures.
- [ ] Add AHCI timeout diagnostics that include port and command slot.
- [ ] Add AHCI reset policy for a wedged port.
- [ ] Add storage device health counters to the `storage` console command.
- [ ] Add read/write byte counters per registered block device.
- [ ] Add partition-level I/O counters.
- [ ] Add bounded request queueing for block devices.
- [ ] Add request cancellation policy for shutdown or device reset.
- [ ] Add cache coherency notes for DMA buffer ownership.
- [ ] Add smoke coverage for multi-sector writes.
- [ ] Add smoke coverage for reads after writes across cluster boundaries.
- [ ] Add documentation for block-device error mapping into filesystem errors.

### Device Discovery

- [ ] Add PCI capability parsing.
- [ ] Add MSI planning for future interrupt routing.
- [ ] Add MSI-X planning for future interrupt routing.
- [ ] Add NVMe discovery planning after AHCI write path stabilization.
- [ ] Add NVMe queue ownership model planning.
- [ ] Add USB keyboard investigation after interrupt routing remains stable.
- [ ] Add USB mouse investigation after interrupt routing remains stable.
- [ ] Add USB mass storage investigation after VFS write semantics are stable.
- [ ] Add stable device path naming for multiple disks.
- [ ] Add hotplug policy documentation even if hotplug is deferred.
- [ ] Add device tree or bus inventory diagnostics to a console command.

### File Descriptor Surface

- [ ] Add `openat` planning or document why absolute paths remain first.
- [ ] Add `mkdir` syscall planning.
- [ ] Add `unlink` syscall planning.
- [ ] Add `rmdir` syscall planning.
- [ ] Add `rename` syscall planning.
- [ ] Add `ftruncate` syscall planning.
- [ ] Add close-on-exec support to descriptor metadata.
- [ ] Add descriptor duplication planning for shell redirection.
- [ ] Add stdin, stdout, and stderr initialization for spawned user programs.
- [ ] Add pipe file descriptor planning beyond the current shell pipeline model.
- [ ] Document descriptor lifetime and ownership across process exit.

## Phase 5: Drivers, Display, Input, And Console UX

### Keyboard Input

- [ ] Move keyboard layout choice behind a small configuration boundary.
- [ ] Add key release handling where useful.
- [ ] Add modifier state reporting for Shift, Control, Alt, and Super.
- [ ] Add Caps Lock state tracking.
- [ ] Add Caps Lock LED update support.
- [ ] Add Num Lock state tracking.
- [ ] Add Scroll Lock state tracking.
- [ ] Add extended scancode support for navigation keys.
- [ ] Add function key decoding.
- [ ] Add keyboard diagnostics for dropped scancodes.
- [ ] Add keyboard diagnostics for unknown scancode sequences.
- [ ] Add user-facing input events for the future window layer.
- [ ] Document the selected default keyboard layout.

### Mouse Input

- [ ] Add mouse wheel packet support.
- [ ] Add horizontal wheel packet planning if the device reports it.
- [ ] Add mouse packet resynchronization diagnostics.
- [ ] Add mouse overflow counters.
- [ ] Add configurable pointer acceleration policy, or document why it is deferred.
- [ ] Add optional double-click state at the UI layer.
- [ ] Add optional drag state at the UI layer.
- [ ] Keep raw mouse movement reporting separate from cursor rendering.
- [ ] Add input smoke diagnostics for mouse packet decoding.
- [ ] Document PS/2 mouse packet variants currently supported.

### Display And Rendering

- [ ] Add a text console with scrolling independent of the graphical overlay.
- [ ] Add damage tracking tests for dirty rectangles.
- [ ] Add renderer diagnostics for dirty rectangle count per frame.
- [ ] Add renderer diagnostics for clipped draw operations.
- [ ] Add a bitmap image rendering path if the UI starts using assets.
- [ ] Add glyph cache invalidation policy if multiple fonts are introduced.
- [ ] Add color palette constants for console themes.
- [ ] Add display mode diagnostics for GOP framebuffer details.
- [ ] Add panic-safe minimal text rendering path.
- [ ] Add documentation for framebuffer ownership and renderer layering.

### Console Commands

- [ ] Add a user-visible command for process spawning once spawn is stable.
- [ ] Add a user-visible command for killing a user process once termination policy exists.
- [ ] Add a command for dumping open file descriptors.
- [ ] Add a command for dumping mount table details with flags.
- [ ] Add a command for dumping interrupt routing state.
- [ ] Add a command for dumping queue overflow counters.
- [ ] Add a command for dumping timer and scheduler quantum diagnostics.
- [ ] Add a command history search shortcut.
- [ ] Add tab completion planning for filesystem paths.
- [ ] Add bounded console output paging for large diagnostics.
- [ ] Add docs for console smoke commands that maintainers should run manually.

### UI Layer Planning

- [ ] Add primitive window and widget layer planning.
- [ ] Define ownership boundaries between input events, widgets, and display rendering.
- [ ] Define focus handling for keyboard input.
- [ ] Define pointer capture handling for drag operations.
- [ ] Define z-order representation for windows.
- [ ] Define invalidation propagation from widgets to dirty rectangles.
- [ ] Add a simple status panel prototype plan.
- [ ] Add a simple file viewer prototype plan.
- [ ] Keep UI state out of low-level input drivers.
- [ ] Document the first UI milestones separately from kernel console milestones.

## Phase 6: Tooling, CI, Tests, And Documentation

### CI And Build Automation

- [ ] Add a headless QEMU smoke test script for CI.
- [ ] Wire storage smoke into CI with serial log artifacts.
- [ ] Add userland build checks to CI for every committed user program.
- [ ] Add kernel `cargo check --target x86_64-unknown-uefi` to CI.
- [ ] Add userland `cargo clippy` target checks to CI.
- [ ] Add architecture boundary checks to CI.
- [ ] Add `cargo fmt --check` for kernel and userland in CI.
- [ ] Cache Rust toolchains and build artifacts safely in CI.
- [ ] Upload QEMU serial logs on CI failure.
- [ ] Upload disk image metadata on CI storage failures.
- [ ] Document the expected CI runtime budget.
- [ ] Add a CI-only timeout around QEMU smoke runs.

### Local Developer Workflow

- [ ] Refresh manual QEMU validation docs with current commands and serial milestones.
- [ ] Link manual QEMU validation docs from `README.md`.
- [ ] Link manual QEMU validation docs from `CONTRIBUTING.md`.
- [ ] Add a one-command local pre-merge verification recipe.
- [ ] Add troubleshooting notes for missing `OVMF.fd`.
- [ ] Add troubleshooting notes for missing QEMU.
- [ ] Add troubleshooting notes for missing nightly Rust components.
- [ ] Add troubleshooting notes for PowerShell execution policy failures.
- [ ] Add docs for regenerating and inspecting `disk.img`.
- [ ] Add docs for reading serial output during manual QEMU runs.
- [ ] Add docs for choosing between `just run` and `just storage-smoke`.

### Architecture Documentation

- [ ] Add a contributor-facing architecture map generated from the current module tree.
- [ ] Add a module ownership table for `src/kernel`.
- [ ] Add a module ownership table for `src/arch/x86_64`.
- [ ] Add a module ownership table for storage drivers.
- [ ] Add a module ownership table for memory management.
- [ ] Add a module ownership table for scheduler and task modules.
- [ ] Add a module ownership table for userland runtime crates.
- [ ] Add diagrams for interrupt dispatch through registered callbacks.
- [ ] Add diagrams for syscall entry and return.
- [ ] Add diagrams for user process lifecycle state transitions.
- [ ] Add diagrams for VFS mount and path traversal.
- [ ] Add diagrams for frame ownership and reclaim paths.

### Test Expansion

- [ ] Add host-side parser fixtures for ELF headers.
- [ ] Add host-side parser fixtures for syscall ABI layout.
- [ ] Add host-side tests for fixed-buffer userland formatting helpers.
- [ ] Add host-side tests for command tokenization.
- [ ] Add smoke tests for invalid user pointers per syscall class.
- [ ] Add smoke tests for file descriptor exhaustion.
- [ ] Add smoke tests for process table exhaustion.
- [ ] Add smoke tests for heap exhaustion.
- [ ] Add smoke tests for map-count exhaustion.
- [ ] Add smoke tests for scheduler fairness.
- [ ] Add smoke tests for console command regressions.
- [ ] Add smoke tests for storage write regressions once writes mutate FAT32.
- [ ] Keep smoke assertions focused on stable serial log lines.

### Release And Maintenance

- [ ] Add a release checklist for verified `master` snapshots.
- [ ] Add a changelog template for milestone summaries.
- [ ] Add a policy for when completed TODO items move into `TODO_COMPLETED.md`.
- [ ] Add a policy for pruning stale future TODO items.
- [ ] Add issue templates for kernel bugs.
- [ ] Add issue templates for driver bugs.
- [ ] Add issue templates for documentation gaps.
- [ ] Add labels or categories matching TODO phases.
- [ ] Add maintainer notes for branch cleanup after direct workflow merges.

## Phase 7: Long-Term Platform, Security, And Multi-Architecture Foundation

### Security Hardening

- [ ] Add kernel address exposure audit for console and serial diagnostics.
- [ ] Add a policy for redacting sensitive addresses in release builds.
- [ ] Add stack canary planning for kernel stacks.
- [ ] Add userland stack canary planning.
- [ ] Add syscall argument fuzzing plan for pointer-heavy syscalls.
- [ ] Add filesystem parser fuzzing plan for byte fixtures.
- [ ] Add ELF parser fuzzing plan for malformed binaries.
- [ ] Add page-table permission audit tooling.
- [ ] Add runtime checks for unexpected writable executable mappings.
- [ ] Add documentation for the current threat model.
- [ ] Add documentation for trusted boot assumptions.
- [ ] Add documentation for DMA trust assumptions.

### SMP And CPU Topology

- [ ] Document why ManaOS remains single-core until scheduler invariants are ready.
- [ ] Add ACPI CPU topology inventory diagnostics.
- [ ] Add Local APIC ID reporting per detected processor.
- [ ] Add bootstrap processor versus application processor role documentation.
- [ ] Plan application processor startup sequence.
- [ ] Plan per-CPU scheduler data.
- [ ] Plan per-CPU interrupt stacks.
- [ ] Plan per-CPU allocator caches or document why they are deferred.
- [ ] Plan cross-CPU TLB shootdown.
- [ ] Plan inter-processor interrupt routing.
- [ ] Add SMP blockers to architecture docs.

### Networking Foundation

- [ ] Add PCI network device discovery planning.
- [ ] Investigate a first supported NIC model for QEMU.
- [ ] Define network driver ownership boundaries.
- [ ] Define packet buffer ownership rules.
- [ ] Define interrupt versus polling mode for first NIC support.
- [ ] Add ARP planning.
- [ ] Add IPv4 planning.
- [ ] Add UDP planning.
- [ ] Add TCP planning or document why it is deferred.
- [ ] Add a minimal network smoke strategy.
- [ ] Document how networking interacts with userland file descriptors.

### Multi-Architecture Readiness

- [ ] Audit `x86_64` assumptions that leak outside `src/arch`.
- [ ] Move architecture-specific constants behind architecture provider APIs.
- [ ] Document the minimum interface a future architecture module must expose.
- [ ] Add architecture-neutral task provider traits where direct function pointers become insufficient.
- [ ] Add architecture-neutral interrupt provider traits where direct callbacks become insufficient.
- [ ] Add architecture-neutral time provider traits where Local APIC assumptions leak.
- [ ] Add architecture-neutral address-space provider documentation.
- [ ] Add compile-time guards for x86_64-only modules.
- [ ] Add docs for a future AArch64 porting checklist.
- [ ] Keep `main.rs` as the composition root for architecture and kernel wiring.

### Packaging And User Experience

- [ ] Add a bootable image packaging plan beyond raw QEMU setup.
- [ ] Add a version banner tied to Git metadata when available.
- [ ] Add build metadata to serial boot logs.
- [ ] Add panic output that includes version metadata.
- [ ] Add a documented command for collecting support logs.
- [ ] Add a documented command for reproducing storage smoke failures.
- [ ] Add screenshots or terminal captures for major milestones.
- [ ] Add a contributor guide for choosing the next TODO slice.
- [ ] Add a milestone map that groups TODO phases into public releases.
- [ ] Add documentation for what must be true before calling the system self-hosting.
