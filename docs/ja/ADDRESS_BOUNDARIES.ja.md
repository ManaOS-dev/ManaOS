# ManaOS Address Boundaries

この文書は [`../ADDRESS_BOUNDARIES.md`](../ADDRESS_BOUNDARIES.md) の日本語版です。
ManaOS で raw address API を棚卸しし、どこに typed physical / virtual address wrapper を
導入するべきかを整理します。

## address type boundary

ManaOS にはまだ raw `u64`、`usize`、pointer、project-owned wrapper、`x86_64` crate の
`PhysAddr` / `VirtAddr` が混在しています。移行方針は、ABI-facing integer は境界に残し、
kernel ownership boundary では型付き address に変換することです。

- syscall ABI argument は `kernel::syscall::dispatch` までは raw `u64` のままです。
  その後、validation 前に user pointer type または scalar value へ変換します。
- ELF file field は raw integer として parse し、mapping 前に user virtual address へ
  変換します。
- UEFI memory map の physical start は boot boundary では raw です。memory management
  内部へ入る前に physical frame / range type へ変換します。
- AHCI register programming では、`DmaPhysicalAddress` を device register boundary でだけ
  low/high half に分割します。
- kernel virtual pointer は、mapping helper が active address space に target range が
  mapped されていることを証明した後にだけ作ります。
- architecture entry-point function pointer は、IDT gate、CPU register、MSR write に下げる前に
  architecture initialization boundary で分類します。

## 実装済みの address boundary

以下は、untyped cross-domain `u64` の代わりに project-owned wrapper を使う境界です。

- `kernel::memory::address::PhysAddr`: physical byte address。
- `kernel::memory::address::VirtAddr`: internal arithmetic 用 virtual byte address。
- address wrapper は final pointer / slice boundary 前に checked `try_as_usize()` lowering を公開します。
  これにより call site は panic-only の `as_usize()` ではなく、明示的な error path を選べます。
- `PhysicalFrameStart` / `FrameCount` / `PhysicalFrameRange`: allocatable 4 KiB frame start、non-zero frame count、contiguous frame ownership。
- `DmaPhysicalAddress`: AHCI descriptor、FIS buffer、command table、PRDT へ program できる physical address。
- `UserVirtualAddress` / `UserVirtualRange`: syscall copy validation 前の non-null user virtual address と byte range。
- `UserReadableRange` / `UserWritableRange` / `UserCString`: copy direction と string policy。
- syscall copy helper は raw pointer / length ABI pair を、page-table permission probe や
  string scan の前に `UserReadableRange`、`UserWritableRange`、または `UserCString`
  constructor へ直接分類します。
- user data page-table permission probe は final raw slice creation 前に
  `UserReadableRange` / `UserWritableRange` を受け取ります。raw `usize` pointer は
  最後の kernel slice / byte read boundary と diagnostic ABI input に閉じます。
- user data page-table permission walk は、page-table flags を見る前に
  `UserVirtualRange` から first / last user page を `UserPageStart` として導出します。
- user address-space permission self-check は kernel probe address を `VirtAddr`、代表 user address を
  `UserVirtualAddress` として受け取り、その後 copy-direction probe range を作ります。
- user address-space template self-check は代表 kernel probe address を `VirtAddr`
  として受け取り、memory API が raw virtual pointer を受け取らないようにします。
- 保存済み kernel address-space root は private atomic storage boundary の中でだけ raw です。
  読み手は CR3 switch や address-space template smoke check に渡す前に、
  すぐ `PhysicalFrameStart` へ分類し直します。
- `UserReadRequest`: syscall ABI pointer classification 後の pending `read` destination を `UserWritableRange` として保持し、scheduler wait state が raw user pointer を保持しないようにします。
- blocking `waitpid`: syscall ABI pointer classification 後の deferred status-write destination を `UserWritableRange` として保持し、scheduler wait completion が raw user pointer を保持しないようにします。
- `UserHeapBreakRequest`: syscall ABI address classification 後の `brk` request。scheduler と heap code は raw break address を受け取りません。
- `UserHeap`: heap-backed mapping の grow/shrink 中、page-aligned mapped extent を `UserPageStart`
  として保持します。
- `UserMappingUnmapRequest`: syscall ABI address classification 後の `munmap` request。scheduler と mapping code は raw unmap start address を受け取りません。
- `UserMappingLength`: syscall ABI validation 後の private `mmap` byte length。scheduler と mapping code は page count を導出するための raw length value を受け取りません。
- `KernelStackGuardFault`: `kernel::interrupt` が raw page-fault address を分類した後の guard / writable / top `VirtAddr`。
- `shared::PageFaultReport`: architecture exception path から registered reporter callback まで
  page fault の fault address、error bits、instruction pointer を保持します。architecture layer は
  raw CR2 と exception-frame value を dispatch 前に分類し、`kernel::interrupt` は
  diagnostics 前に virtual address field を `VirtAddr` へ変換します。
- `arch::x86_64::SyscallEntryAddress`: `SYSCALL` LSTAR MSR に program する virtual entry target。
  composition root は architecture initialization に typed value を渡し、raw number への lowering は
  final MSR write boundary の中だけに閉じます。
- `arch::x86_64::interrupt_descriptor_table` は assembly timer interrupt entry target を
  `InterruptEntryAddress` として分類してから IDT gate へ下ろします。
- user task kernel stack top は scheduler handoff path と task architecture facade では `VirtAddr` として保持し、facade が registered architecture installer を呼ぶ境界と `SYSCALL` entry stack-top atomic の境界でだけ raw `u64` へ下ろします。
- kernel task stack top は `TaskContext::from_stack(...)` に `VirtAddr` として渡し、
  constructor が stack pointer を align した後、private assembly-facing context layout へ下ろします。
- user trap-frame storage address は `kernel::task::record_current_user_trap_frame(...)`
  の前に `VirtAddr` へ分類し、scheduler metadata が raw kernel stack address を受け取らないようにします。
- `UserTrapFrame` は architecture restore ABI のために `repr(C)` register field を raw のまま保ちます。
  ただし kernel diagnostics と `execve` publication は、user RIP/RSP を formatting する前に
  typed `UserVirtualAddress` accessor で読みます。
- scheduler task snapshot は last resume address-space root を `PhysicalFrameStart`、last resume kernel stack top を `VirtAddr` として保持し、console / smoke output formatting の境界でだけ raw number へ下ろします。
- user virtual-memory task snapshot は `brk` heap base、current break、private mapping next-start を `UserVirtualAddress` として保持し、console / smoke output formatting の境界でだけ raw number へ下ろします。
- `user_stack::allocate_and_map_user_page(...) -> PhysicalFrameStart`。
- `user_stack::map_user_range(...)` の internal user virtual / physical frame boundary。
- `paging::map_kernel_mmio_range(...)` の MMIO physical base `PhysAddr` と mapped page coverage の `PageCount`。
  identity-mapped page start は page-table mutation 前に `PhysicalFrameStart` として分類します。
- PCI AHCI discovery から controller initialization / HBA MMIO mapping までの BAR5 `PhysAddr`。
- APIC routing configuration は Local APIC / IOAPIC MMIO physical base を
  `ApicMmioAddress` として保持し、Local APIC / IOAPIC / Local APIC timer register
  wrapper が pointer-sized MMIO address へ下ろす直前まで raw に戻しません。
- Local APIC timer の calibration / active status snapshot は、timer MMIO base を
  `ApicMmioAddress` として保持します。private atomic slot は publication boundary としてだけ
  raw のまま残し、boot diagnostics は serial output の直前だけ typed value を `u64` へ下ろします。
- `PhysicalFrameAllocator::add_region(...)` と `reserve_region*` の `PhysAddr` physical start と `FrameCount` frame count。
- ACPI root pointer、root table、MADT、Local APIC、IOAPIC diagnostics は、
  firmware / ACPI byte field から parse した後の physical address を `PhysAddr` として保持します。
  boot diagnostics は serial output 直前だけ raw に下ろし、APIC routing setup は architecture-owned
  `ApicMmioAddress` を構築する直前だけ raw に下ろします。
- `AhciDmaBuffers` 内部の `DmaPhysicalAddress`。storage smoke は command-list、
  received-FIS、command-table、data-buffer setup が final register split 直前まで
  typed DMA address boundary に留まることを assert します。
- `StorageDataAddress`: generic storage parsing に渡す active DMA data buffer。
- `FramebufferPhysicalRange`: graphics-mode framebuffer physical range。
- `KernelVirtualAddress`: identity-mapped kernel virtual address。
- `PageCount`: virtual range reservation、user stack allocation、private user mapping tracking、paging helper byte range mapping 前の non-zero 4 KiB page count。
- `KernelVirtualRange`: future dynamic mapping 用 higher-half kernel virtual range。
- `KernelVirtualRangeAllocator::new(...)` と `allocate_pages(...)` は `PageCount` を受け取ります。
- `process::UserProgramSpawnRequest::new(...)` と `user_stack::allocate_user_stack(...)` は user stack page mapping 前に `PageCount` を受け取ります。
- `UserMappings` は private mapping record start を `UserPageStart`、record count を `PageCount`
  として保持します。`map_private(...)` は typed page count を返し、`unmap_range(...)` は
  typed unmap request を受け取ってから scheduler diagnostics 用の typed page count を返します。
  automatic placement search cursor と split record start も record update / diagnostics formatting
  前まで `UserPageStart` として保持します。internal overlap / containment helper は
  `UserPageStart` start/end boundary を持つ private typed mapping range を渡し、raw start/end
  pair を渡しません。
- `task::UserMappingRequest` は requested `mmap` address を `UserMappingPlacement`
  としてだけ保持します。scheduler diagnostics の requested address 表示は typed placement から導出し、
  raw syscall address を保持しません。
- ELF entry point は header validation 直後に `UserVirtualAddress` へ変換します。
  loader metadata と entry segment membership check は raw ELF header field を再利用せず、
  typed value を使います。
- ELF heap start は各 load segment end を user page へ align した後、
  `UserPageStart` として集計します。`LoadedElf` は final heap start を
  `UserVirtualAddress` として公開します。
- ELF load segment の memory range は `UserVirtualRange` へ変換し、mapping / page-copy
  helper が使う first/last mapped page は `UserPageStart` へ変換します。storage smoke は
  segment、page、file-backed range の typed marker を assert します。
- `UserAddressSpace`: task-owned user PML4 root。
- `paging::map_kernel_writable_no_execute_range(...)`: reserved virtual range と owned physical frame range を mapped kernel pages にする境界。
- `paging::unmap_kernel_range_and_free_frames(...)`: kernel virtual mapping を外し、owner check 後に physical frame を返す境界。

## まだ raw address が残りやすい場所

raw address が残るべき場所と、早めに wrapper 化すべき場所を分けて考えます。

### Boot と composition root

`src/main.rs` では、architecture ABI に渡す function pointer などが raw value になりやすいです。
syscall entry function pointer は architecture initialization 前に
`arch::x86_64::SyscallEntryAddress` へ変換し、raw LSTAR value は `init_syscall(...)`
の中でだけ作ります。
ただし user entry、user stack、`argv`、`envp` は、`UserTaskContext` が private `repr(C)` ABI
layout へ落とす直前まで typed に保ちます。

### Frame allocation と heap

UEFI memory-map descriptor は firmware ABI record なので raw physical start を持ちます。
boot composition root はそれを `PhysAddr` へ wrap してから allocator に登録します。

`PhysicalFrameAllocator` は single frame では `PhysicalFrameStart`、contiguous frame では
`FrameCount` を入力に取り、`PhysicalFrameRange` を返します。

### Paging

page-table helper は alignment、range end、page walk のために local `PhysAddr` / `VirtAddr`
arithmetic を行い、mapper boundary で `x86_64` address type へ変換します。
`KernelVirtualRangeAllocator` は managed virtual range と個別の higher-half virtual reservation に
`PageCount` を使います。

### User memory

user stack allocation、prepared stack pointer、ELF entry point、syscall copy helper は、
raw ABI argument から typed user pointer へ変換してから validation へ進みます。
keyboard-backed `read` の pending wait は、task address space が再び active になるまで destination を
`UserWritableRange` として保持し、copy 前に page-table permission を再検証します。
blocking `waitpid` の pending wait は、parent task address space が再び active になるまで optional status destination を
`UserWritableRange` として保持し、wait status 書き込み前に page-table permission を再検証します。
`paging` と per-process `UserAddressSpace` の user data permission check は
`UserReadableRange` または `UserWritableRange` を受け取ります。syscall pointer classification 後に
raw pointer / length pair を受け取りません。
syscall buffer helper は `UserReadableRange`、`UserWritableRange`、`UserCString` の
syscall constructor を使うため、raw pointer / length pair は copy-direction classification を
越えて漏れません。
permission check の page walk boundary は `UserVirtualRange` から `UserPageStart` として導出し、
active / per-process page-table probe が user page start を raw virtual address として扱わないようにします。
per-process address-space permission self-check は kernel probe を `VirtAddr`、user probe を
`UserVirtualAddress` のまま保持し、raw `usize` への lowering は final diagnostics と kernel slice
construction の境界に限定します。
user address-space template self-check は代表 kernel probe を `VirtAddr` として保持します。
boot smoke call site だけが architecture pointer lowering と checked numeric conversion を行ってから
memory API に渡します。
保存済み kernel address-space root は private `AtomicU64` boundary を通るためにだけ raw integer として保持します。
kernel address-space switch と template smoke check は、それを `PhysicalFrameStart` として
読み直してから page-table helper へ渡します。
`brk` request は `sys_brk` で raw ABI value を current-break query または validated user virtual address に分類してから `UserHeap` へ渡します。
heap growth / shrink helper は aligned mapped-end boundary を `UserPageStart` として保持し、
comparison や diagnostics の直前だけ raw number へ下げます。
kernel stack guard-fault lookup は `kernel::interrupt` が `shared::PageFaultReport` を受け取り、
page-fault virtual address を `VirtAddr` へ分類してから scheduler boundary へ渡します。
user entry と timer-resume の handoff は、選択した user task の kernel stack top を task architecture facade まで `VirtAddr` として保持します。facade 内の architecture provider call と `SYSCALL` entry stack-top atomic が残る raw lowering point です。
syscall / timer trap-frame storage address は architecture/shared ABI の capture point だけ raw のままです。
kernel interrupt / syscall bridge は captured `UserTrapFrame` を task scheduler に記録する前に
`VirtAddr` へ変換します。
user trap-frame の RIP/RSP field は fixed `repr(C)` resume frame の中では raw のままです。
kernel logging、diagnostics、`execve` publication は typed `UserVirtualAddress` accessor を使ってから、
output の境界で raw number へ下げます。
`execve` image publication は replacement heap start も `UserVirtualAddress` のまま保持し、
serial diagnostics が numeric address を必要とする直前だけ raw number へ下げます。
scheduler task snapshot は last resume address-space root を `PhysicalFrameStart`、last resume kernel stack top を `VirtAddr` として保持し、console / serial smoke diagnostics の formatting 時だけ raw number にします。
user virtual-memory task snapshot は heap base、heap break、private mapping next-start address を `UserVirtualAddress` として保持し、console / serial smoke diagnostics の formatting 時だけ raw number にします。
user stack allocation の page count は `PageCount` で分類してから frame allocation と stack slot mapping に進みます。
private user mapping は syscall byte length を ABI validation 後に `PageCount` へ変換し、mapping record、successful allocation、unmap result で typed page count を使います。
その syscall byte length は `UserMappingLength` として scheduler-owned `mmap` request に保持し、raw length は request construction / rejection に閉じます。
mapping record start と automatic placement cursor は `UserPageStart` のまま保持するため、private mapping record と次の private mapping search は unaligned raw virtual address を保持しません。
internal overlap / containment helper は page-aligned start と exclusive-end boundary を持つ private typed mapping range を渡し、comparison の直前だけ address を raw number へ下げます。
`munmap` または fixed replacement が record を分割するとき、右側 record start は record table 更新中も
`UserPageStart` として保持します。
`mmap` request は fixed requested address を `UserMappingPlacement` 内の `UserPageStart`
として保持します。syscall raw requested address は placement の選択または request rejection にだけ使います。
`munmap` request は syscall ABI argument を `UserMappingUnmapRequest` へ分類してから scheduler と mapping tracker へ渡します。
`UserTaskContext` の raw register layout は architecture entry ABI のために private に保ちます。
compile-time layout assertion が private layout を守り、storage smoke は diagnostics が pointer を
serial output 用に lower する前の typed entry-argument handoff を assert します。

### ELF loading

ELF file の program header field は file format 上 raw `u64` です。loader validation が成功した
segment だけを local typed `UserVirtualRange` へ変換して mapping と copy を行います。
ELF entry point は header validation 後に `UserVirtualAddress` として保持し、
entry segment membership check でも typed value を使います。
ELF loader は `LoadedElf::heap_start()` を導出する間、最大の page-aligned segment end を
`UserPageStart` として保持します。
ELF load segment の file-backed payload range は page-copy 計算の直前まで
`UserVirtualRange` として保持します。raw offset は checked file/page overlap arithmetic の局所変数へ閉じます。
ELF load segment の page walk は `LoadSegmentRange` から得た `UserPageStart` boundary を受け取り、
validation 後の raw segment virtual address を mapping helper へ渡し直しません。

### Storage と AHCI DMA

storage parser / block-device path は `StorageDataAddress` を使います。raw pointer conversion は、
block device が active DMA data buffer を埋めた後に sector slice を作る境界へ閉じます。
AHCI DMA setup は、command-list、received-FIS、command-table、data buffer address を
device register が low/high half を必要とする直前まで `DmaPhysicalAddress` として保持します。
storage smoke はこの typed DMA setup boundary を assert します。

## 推奨 wrapper

今後も小さい差分で以下を使い分けます。

- `PhysAddr`: physical byte address。
- `PhysAddr`: serial diagnostics または architecture-specific APIC MMIO wrapper に渡す前の
  ACPI table / interrupt-controller physical address。
- `VirtAddr`: virtual byte address。
- `PhysicalFrameStart`: 4 KiB aligned physical frame start。
- `FrameCount`: frame allocator API に渡す non-zero physical frame count。
- `PhysicalFrameRange`: frame start と frame count。
- `FramebufferPhysicalRange`: active framebuffer physical range。
- `KernelVirtualAddress`: mapped kernel virtual address。
- `PageCount`: kernel virtual range allocator API、user stack API、user mapping API、paging helper API に渡す non-zero 4 KiB page count。
- `KernelVirtualRange`: reserved higher-half kernel virtual range。
- `UserAddressSpace`: user page-table root。
- `UserVirtualAddress`: non-null user pointer / ELF virtual address。
- `UserVirtualRange`: non-empty validated user pointer range。
- `UserReadableRange` / `UserWritableRange`: syscall copy direction。
- `UserReadRequest`: raw syscall pointer classification 後に scheduler が保持する pending `read` destination。
- `UserWritableRange`: raw syscall pointer classification 後に scheduler が保持する blocking `waitpid` status destination。
- `UserCString`: NUL validation 前の syscall string candidate。
- `UserMappingUnmapRequest`: syscall ABI classification 後の private `munmap` request。
- `UserMappingLength`: syscall ABI classification 後の private `mmap` length。
- `VirtAddr`: task architecture facade / `SYSCALL` entry raw boundary 前の scheduler-owned user task kernel stack top handoff。
- `PhysicalFrameStart` / `VirtAddr`: console / smoke output formatting 前の scheduler resume handoff diagnostic snapshot。
- `UserVirtualAddress`: console / smoke output formatting 前の user virtual-memory scheduler snapshot。
- `DmaPhysicalAddress`: device descriptor に program できる physical address。
- `StorageDataAddress`: generic storage parsing に渡す active DMA data buffer。
- `ApicMmioAddress`: architecture register access が pointer-sized address へ下ろす前の
  APIC-family MMIO physical base。
- `ApicMmioAddress`: boot diagnostics が serial output 用に下ろす前の Local APIC timer
  calibration / active status snapshot。
- `SyscallEntryAddress`: x86_64 `SYSCALL` LSTAR に program する architecture-owned virtual entry point。
- `InterruptEntryAddress`: x86_64 IDT gate に program する architecture-owned interrupt entry point。
- `PageFaultReport` / `PageFaultAddress` / `PageFaultErrorBits` /
  `PageFaultInstructionPointer`: kernel diagnostics が virtual address を `VirtAddr` として分類する前の
  shared page-fault callback boundary。

## 移行順

1. frame allocator return value を wrap する。
2. UEFI memory-map physical start と contiguous physical frame range を wrap する。
3. ELF loading と user stack setup の user virtual address を wrap する。
4. syscall dispatch が raw ABI から変換した後の user pointer argument を wrap する。
5. AHCI DMA physical address を wrap し、register splitting は hardware boundary に閉じる。
6. MMIO と framebuffer physical range を regular RAM と分けて wrap する。
7. boundary wrapper が揃ってから internal raw address arithmetic を減らす。
8. assembly boundary の ABI field は raw のまま維持しつつ、constructor と caller は typed に保つ。
