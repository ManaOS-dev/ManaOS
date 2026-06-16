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

## 実装済みの address boundary

以下は、untyped cross-domain `u64` の代わりに project-owned wrapper を使う境界です。

- `kernel::memory::address::PhysAddr`: physical byte address。
- `kernel::memory::address::VirtAddr`: internal arithmetic 用 virtual byte address。
- `PhysicalFrameStart` / `FrameCount` / `PhysicalFrameRange`: allocatable 4 KiB frame start、non-zero frame count、contiguous frame ownership。
- `DmaPhysicalAddress`: AHCI descriptor、FIS buffer、command table、PRDT へ program できる physical address。
- `UserVirtualAddress` / `UserVirtualRange`: syscall copy validation 前の non-null user virtual address と byte range。
- `UserReadableRange` / `UserWritableRange` / `UserCString`: copy direction と string policy。
- user data page-table permission probe は final raw slice creation 前に
  `UserReadableRange` / `UserWritableRange` を受け取ります。raw `usize` pointer は
  最後の kernel slice / byte read boundary と diagnostic ABI input に閉じます。
- `UserReadRequest`: syscall ABI pointer classification 後の pending `read` destination を `UserWritableRange` として保持し、scheduler wait state が raw user pointer を保持しないようにします。
- blocking `waitpid`: syscall ABI pointer classification 後の deferred status-write destination を `UserWritableRange` として保持し、scheduler wait completion が raw user pointer を保持しないようにします。
- `UserHeapBreakRequest`: syscall ABI address classification 後の `brk` request。scheduler と heap code は raw break address を受け取りません。
- `UserMappingUnmapRequest`: syscall ABI address classification 後の `munmap` request。scheduler と mapping code は raw unmap start address を受け取りません。
- `KernelStackGuardFault`: `kernel::interrupt` が raw page-fault address を分類した後の guard / writable / top `VirtAddr`。
- user task kernel stack top は scheduler handoff path では `VirtAddr` として保持し、registered architecture installer と `SYSCALL` entry stack-top atomic の境界でだけ raw `u64` へ下ろします。
- scheduler task snapshot は last resume address-space root を `PhysicalFrameStart`、last resume kernel stack top を `VirtAddr` として保持し、console / smoke output formatting の境界でだけ raw number へ下ろします。
- user virtual-memory task snapshot は `brk` heap base、current break、private mapping next-start を `UserVirtualAddress` として保持し、console / smoke output formatting の境界でだけ raw number へ下ろします。
- `user_stack::allocate_and_map_user_page(...) -> PhysicalFrameStart`。
- `user_stack::map_user_range(...)` の internal user virtual / physical frame boundary。
- `paging::map_kernel_mmio_range(...)` の MMIO physical base `PhysAddr` と mapped page coverage の `PageCount`。
- PCI AHCI discovery から controller initialization / HBA MMIO mapping までの BAR5 `PhysAddr`。
- `PhysicalFrameAllocator::add_region(...)` と `reserve_region*` の `PhysAddr` physical start と `FrameCount` frame count。
- `AhciDmaBuffers` 内部の `DmaPhysicalAddress`。
- `StorageDataAddress`: generic storage parsing に渡す active DMA data buffer。
- `FramebufferPhysicalRange`: graphics-mode framebuffer physical range。
- `KernelVirtualAddress`: identity-mapped kernel virtual address。
- `PageCount`: virtual range reservation、user stack allocation、private user mapping tracking、paging helper byte range mapping 前の non-zero 4 KiB page count。
- `KernelVirtualRange`: future dynamic mapping 用 higher-half kernel virtual range。
- `KernelVirtualRangeAllocator::new(...)` と `allocate_pages(...)` は `PageCount` を受け取ります。
- `process::UserProgramSpawnRequest::new(...)` と `user_stack::allocate_user_stack(...)` は user stack page mapping 前に `PageCount` を受け取ります。
- `UserMappings` は private mapping record count を `PageCount` として保持します。`map_private(...)` は typed page count を返し、`unmap_range(...)` は typed unmap request を受け取ってから scheduler diagnostics 用の typed page count を返します。automatic placement search cursor と split record start は record update / diagnostics formatting 前まで `UserPageStart` として保持します。
- `task::UserMappingRequest` は requested `mmap` address を `UserMappingPlacement`
  としてだけ保持します。scheduler diagnostics の requested address 表示は typed placement から導出し、
  raw syscall address を保持しません。
- ELF entry point は header validation 直後に `UserVirtualAddress` へ変換します。
  loader metadata と entry segment membership check は raw ELF header field を再利用せず、
  typed value を使います。
- ELF heap start は各 load segment end を user page へ align した後、
  `UserPageStart` として集計します。`LoadedElf` は final heap start を
  `UserVirtualAddress` として公開します。
- `UserAddressSpace`: task-owned user PML4 root。
- `paging::map_kernel_writable_no_execute_range(...)`: reserved virtual range と owned physical frame range を mapped kernel pages にする境界。
- `paging::unmap_kernel_range_and_free_frames(...)`: kernel virtual mapping を外し、owner check 後に physical frame を返す境界。

## まだ raw address が残りやすい場所

raw address が残るべき場所と、早めに wrapper 化すべき場所を分けて考えます。

### Boot と composition root

`src/main.rs` では、architecture ABI に渡す function pointer などが raw value になりやすいです。
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
`brk` request は `sys_brk` で raw ABI value を current-break query または validated user virtual address に分類してから `UserHeap` へ渡します。
kernel stack guard-fault lookup は `kernel::interrupt` で raw architecture page-fault address を `VirtAddr` へ分類してから scheduler boundary へ渡します。
user entry と timer-resume の handoff は、選択した user task の kernel stack top を `VirtAddr` として保持します。architecture provider call と `SYSCALL` entry stack-top atomic が残る raw lowering point です。
scheduler task snapshot は last resume address-space root を `PhysicalFrameStart`、last resume kernel stack top を `VirtAddr` として保持し、console / serial smoke diagnostics の formatting 時だけ raw number にします。
user virtual-memory task snapshot は heap base、heap break、private mapping next-start address を `UserVirtualAddress` として保持し、console / serial smoke diagnostics の formatting 時だけ raw number にします。
user stack allocation の page count は `PageCount` で分類してから frame allocation と stack slot mapping に進みます。
private user mapping は syscall byte length を ABI validation 後に `PageCount` へ変換し、mapping record、successful allocation、unmap result で typed page count を使います。
automatic placement cursor は `UserPageStart` のまま保持するため、次の private mapping search は unaligned raw virtual address を保持しません。
`munmap` または fixed replacement が record を分割するとき、右側 record start は record table 更新前に
`UserPageStart` として分類します。
`mmap` request は fixed requested address を `UserMappingPlacement` 内の `UserPageStart`
として保持します。syscall raw requested address は placement の選択または request rejection にだけ使います。
`munmap` request は syscall ABI argument を `UserMappingUnmapRequest` へ分類してから scheduler と mapping tracker へ渡します。
`UserTaskContext` の raw register layout は architecture entry ABI のために private に保ちます。

### ELF loading

ELF file の program header field は file format 上 raw `u64` です。loader validation が成功した
segment だけを local typed `UserVirtualRange` へ変換して mapping と copy を行います。
ELF entry point は header validation 後に `UserVirtualAddress` として保持し、
entry segment membership check でも typed value を使います。
ELF loader は `LoadedElf::heap_start()` を導出する間、最大の page-aligned segment end を
`UserPageStart` として保持します。
ELF load segment の file-backed payload range は page-copy 計算の直前まで
`UserVirtualRange` として保持します。raw offset は checked file/page overlap arithmetic の局所変数へ閉じます。

### Storage と AHCI DMA

storage parser / block-device path は `StorageDataAddress` を使います。raw pointer conversion は、
block device が active DMA data buffer を埋めた後に sector slice を作る境界へ閉じます。

## 推奨 wrapper

今後も小さい差分で以下を使い分けます。

- `PhysAddr`: physical byte address。
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
- `VirtAddr`: architecture / `SYSCALL` entry boundary 前の scheduler-owned user task kernel stack top handoff。
- `PhysicalFrameStart` / `VirtAddr`: console / smoke output formatting 前の scheduler resume handoff diagnostic snapshot。
- `UserVirtualAddress`: console / smoke output formatting 前の user virtual-memory scheduler snapshot。
- `DmaPhysicalAddress`: device descriptor に program できる physical address。
- `StorageDataAddress`: generic storage parsing に渡す active DMA data buffer。

## 移行順

1. frame allocator return value を wrap する。
2. UEFI memory-map physical start と contiguous physical frame range を wrap する。
3. ELF loading と user stack setup の user virtual address を wrap する。
4. syscall dispatch が raw ABI から変換した後の user pointer argument を wrap する。
5. AHCI DMA physical address を wrap し、register splitting は hardware boundary に閉じる。
6. MMIO と framebuffer physical range を regular RAM と分けて wrap する。
7. boundary wrapper が揃ってから internal raw address arithmetic を減らす。
8. assembly boundary の ABI field は raw のまま維持しつつ、constructor と caller は typed に保つ。
