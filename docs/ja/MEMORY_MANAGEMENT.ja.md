# ManaOS Memory Management

この文書は [`../MEMORY_MANAGEMENT.md`](../MEMORY_MANAGEMENT.md) の日本語版です。
ManaOS の physical frame ownership、reusable frame allocation、dynamic kernel virtual
mapping の不変条件を記録します。

## `PhysicalFrameAllocator` の現在の call site

`PhysicalFrameAllocator` は、現時点で唯一の physical frame source です。boot composition root
から、physical memory を必要とする subsystem へ渡されます。

- `src/main.rs`: UEFI conventional memory region を登録し、boot、storage、ELF loading、
  user smoke setup へ allocator を渡します。
- `src/kernel/boot/mod.rs`: paging 有効化後に kernel heap を割り当てます。
- `src/kernel/memory/paging.rs`: page-table frame を割り当て、memory map range、
  framebuffer page、MMIO page を identity map または kernel map します。
- `src/kernel/memory/address_space.rs`: user PML4 root を割り当て、kernel mapping を共有し、
  process user window を clear し、kernel/user address space 間で CR3 を切り替えます。
- `src/kernel/memory/user_stack.rs`: specific user address space に user stack page と
  user page-table page を割り当てます。
- `src/kernel/elf/loader.rs`: user ELF `PT_LOAD` segment 用 frame を割り当てます。
- `src/kernel/driver/storage/advanced_host_controller_interface/dma.rs`: AHCI command、
  FIS、command-table、data DMA buffer を割り当てます。
- `controller.rs` / `host.rs`: controller setup と MMIO mapping setup へ allocator を渡します。
- `src/kernel/driver/storage/mod.rs`: storage probing と persistent block-device setup へ
  allocator を渡します。

## allocator invariants

physical frame allocator は、以下の前提に依存します。

- `ExitBootServices` 前に UEFI `CONVENTIONAL` memory だけを登録します。
- memory registration API は `PhysAddr` start を受け取り、virtual address を physical range model
  に渡せないようにします。
- `PhysicalFrameStart` construction は `PhysAddr` だけを受け取るため、hardware register や
  page table から読んだ raw address は境界で physical address として分類してから渡します。
- tracked allocator range は physical start を `PhysAddr` として保持し、sort や byte-distance
  calculation が必要な箇所だけ raw number に下げます。
- `FrameCount` construction は zero count と byte-length overflow を拒否してから、frame
  allocator API に contiguous frame count を渡します。
- `PageCount` construction は zero count と byte-length overflow を拒否してから、kernel
  virtual range allocator API、user stack API、private user mapping API、paging helper API に 4 KiB page count を渡します。
- `UserVirtualAddress` construction は `VirtAddr` だけを受け取るため、syscall や ELF loader の
  raw address field は user address wrapper に入る前に分類してから渡します。
- user page mapping/unmapping API は `UserPageStart` を要求するため、page table を変更する前に
  4 KiB user-page alignment を確定します。
- `KernelVirtualRange` start は `KernelPageStart` 必須のため、dynamic kernel virtual reservation は
  unaligned higher-half start を保持しません。
- user permission probe は active / per-process page table を歩く前に、
  `UserVirtualRange` から first / last page boundary を `UserPageStart` として導出します。
- user address-space template self-check は代表 kernel probe を `VirtAddr` として受け取り、
  fresh user PML4 root が kernel mapping を user-accessible にせず共有していることを確認します。
- 保存済み kernel address-space root は private atomic storage slot の中でだけ raw です。
  すべての reader は CR3 switch や address-space template smoke check に使う前に
  `PhysicalFrameStart` へ分類し直します。
- scheduler-owned kernel stack metadata は guard-page / writable-page start を
  `KernelPageStart` として保持し、guard-fault diagnostics が serial / console output 用に
  下ろす前に page alignment を表現します。
- syscall buffer helper は raw pointer / length ABI argument を
  `UserReadableRange`、`UserWritableRange`、または `UserCString` へ分類してから、
  copy direction を page-table permission probe や string scan に渡します。
- registered range は 4 KiB page に正規化し、physical address zero を避けます。
- registered range は sort し、隣接 range は merge します。
- allocation は tracked free range を scan し、owner が release するまで同じ physical frame を
  2回返しません。
- deallocation は expected owner を要求します。owner mismatch と double free は拒否します。
- contiguous allocation は1つの registered range 内でだけ保証されます。
- 返された physical address は、frame zeroing、page table construction、AHCI DMA へ渡す場面で
  identity mapped されていると仮定します。
- caller は、boot 終了または明示的な ownership transfer まで frame を exclusive owner として扱います。

## reusable physical frame allocator design

allocator は physical memory を frame range と explicit state で表現します。

- `Reserved`: allocate できません。physical address zero、firmware non-conventional memory、
  kernel image、boot module、page table、framebuffer/MMIO、device-owned DMA buffer、guard page など。
- `Free`: allocate 可能な conventional frame。
- `Used`: ちょうど1つの subsystem、user address space、page table、heap、DMA buffer、
  boot structure が所有する frame。

所有権ルール:

- `Free -> Used` は allocator 経由でのみ行います。
- `Used -> Free` は owner が明示的に release し、page table、DMA descriptor、heap span、
  task metadata が参照していないことが必要です。
- `Free -> Reserved` は guard page や hardware range のために行えます。
- `Reserved -> Free` は temporary boot-only reservation で最後の利用者が消えたと証明された場合だけです。
- DMA frame は device がアクセスできる間 `Used` または `Reserved` のままです。
- page-table frame は owning address space が完全に破棄されるまで `Used` のままです。
- user memory frame は process lifecycle が owning address space を unmap/release するまで `Used` です。

contiguous allocation は、hardware や ABI が物理連続を本当に必要とする場合だけ使います。

## user address-space ownership model

user task は separate address-space root を所有します。

- kernel heap frame は kernel lifetime 全体で kernel-owned です。
- kernel page-table frame は active になり得る間 free してはいけません。
- user address-space PML4 frame は task/process-owned で、kernel PML4 entry を共有しつつ
  process user PML4 window を clear します。
- user ELF segment frame は user-task-owned で、owning user address space にだけ map されます。
- user stack frame は user-task-owned で、guard page は unmapped のままです。
- user heap frame は `brk` で active user address space に map され、address space destruction で返されます。
- anonymous user mapping frame は ManaOS の `mmap` subset で map され、`munmap` または
  address-space destruction で返されます。
- kernel stack frame は task-owned で、higher-half kernel virtual range に map されます。
- AHCI DMA frame は storage-driver-owned で、controller が access できる間は再利用できません。
- framebuffer と MMIO range は hardware-owned mapping であり、regular RAM として渡してはいけません。
- identity mapping は ownership ではなく mapping policy です。

## identity mapping audit

現在 identity mapping を前提にしている主な場所:

- `paging.rs` の page-table construction と CR3 table access。
- `paging.rs` の MMIO / framebuffer mapping setup。
- `dma.rs` の AHCI DMA buffer zeroing。
- explicit user address space に map しながら physical frame 経由で user stack を準備する path。
- allocated physical frame へ ELF segment bytes を copy する path。

縮小は段階的に進めます。

1. physical-memory window または recursive mapping ができるまで、page-table frame の identity mapping を維持します。
2. storage code が explicit kernel virtual mapping で DMA buffer initialization を行うまで、DMA buffer の identity mapping を維持します。
3. ELF loading と user stack setup が explicit kernel mapping 経由になるまで、user frame の identity mapping を維持します。
4. framebuffer と MMIO は hardware range なので、regular frame allocator ownership とは分けて変換します。

## kernel virtual range reservation

kernel には dynamic mapping 用の reusable higher-half virtual address range allocator があります。
これは virtual address を予約するだけで、page-table mapping、unmapping、physical frame ownership は
別責務です。

allocator は managed higher-half start に `KernelPageStart`、managed range construction と
個別 allocation に `PageCount` を受け取るため、caller は virtual address space を予約する前に
raw page start と raw page count を分類します。
user stack allocation も `PageCount` を受け取るため、spawn / execve caller は stack size を
page count として分類してから frame allocation と stack slot mapping に進みます。
private user mapping は syscall byte length を ABI validation 後に `UserMappingLength` へ変換します。
typed length は rounded `PageCount` を保持し、successful allocation と unmap の page count を
scheduler diagnostics の aggregate counter に畳み込む直前まで typed のまま保ちます。
MMIO identity mapping は byte range を `PageCount` へ変換してから 4 KiB page を歩きます。
APIC smoke log は Local APIC と IOAPIC register mapping の returned typed page count を記録します。

guarded stack work では以下のように使います。

- `N + 1` virtual pages を予約する。
- lowest page を guard page として unmapped のままにする。
- 残りの page を kernel-only writable non-executable page として map する。

dynamic kernel mapping には generic unmap path があります。

- `paging::map_kernel_writable_no_execute_range(...)`: owned physical range を reserved kernel virtual range へ map します。
- `paging::unmap_kernel_range_and_free_frames(...)`: mapping を外し、expected owner が一致する場合だけ frame を返します。
- `KernelVirtualRangeAllocator::free_pages(...)`: mapping が消えた後、virtual range を再利用可能にします。

## `brk` と private `mmap`

`brk` は syscall-time user heap growth の最初の path です。syscall boundary は raw ABI argument を
`UserHeapBreakRequest` へ分類してから scheduler と heap code へ渡します。ELF loader が最高位
`PT_LOAD` segment の後ろに page-aligned heap start を報告し、scheduler が current heap break を
各 user task runtime に保存します。heap growth は writable non-executable user heap page を map し、
growth / shrink helper は mapped-end boundary を `UserPageStart` として保持します。comparison と
diagnostics の直前だけ raw number に下げます。shrink は不要になった heap page を unmap して
`UserHeap` owner pool へ返します。

private `mmap` は syscall-time user memory の2つ目の path です。現在の ABI は以下を扱います。

- `addr = 0` の automatic anonymous mapping。
- `MAP_FIXED_NOREPLACE` による non-overlapping fixed anonymous mapping。
- `MAP_FIXED` による private mapping replacement。
- current VFS file descriptor からの read-only file-private mapping。

executable mapping は、実行可能 mapping の ownership と cache rule が定義されるまで拒否します。
mapping request は fixed requested address を `UserMappingPlacement` 内の `UserPageStart`
として保持し、scheduler diagnostics の raw 表示値は typed placement から導出します。
automatic placement の next search cursor も `UserPageStart` として保持し、
allocation diagnostics の formatting 前まで page-aligned typed value を保ちます。
requested mapping length は `UserMappingLength` 内に保持し、scheduler と mapping table はそこから page count を導出します。
record split は record table を更新する前に右側 start を `UserPageStart` として分類します。
internal overlap / containment check は `UserPageStart` start と exclusive-end boundary を持つ
private typed mapping range を使い、comparison の直前だけ address を raw number へ下げます。
mapping record は start を `UserPageStart`、non-zero page count を `PageCount` として保持し、
successful unmap result も non-zero page count を `PageCount` として保持します。
lifetime / diagnostic total は 0 になり得るため `u64` counter のままです。

user pointer copy helper と per-process address-space probe は、
`UserVirtualRange::first_page_start()` / `last_page_start()` から permission-check page walk を導出します。
walker は `x86_64` page-table translation 用に raw number へ下げる前に、
`UserPageStart` boundary を比較します。
syscall copy helper layer は readable、writable、C-string candidate range を
direction-specific constructor 経由で作るため、raw ABI pointer / length pair は
lower copy helper に届く前に分類されます。
address-space template self-check は代表 kernel probe を `VirtAddr` として受け取るため、
function pointer を numeric address に下ろす場所は boot smoke call site に閉じます。

## user address spaces

`kernel::memory::address_space::UserAddressSpace` は user PML4 root を含む physical frame を所有します。
creation は active kernel template を copy し、linked user program range と user stack slot range を
覆う PML4 entries `128..256` を clear します。
scheduler の user-entry / timer-resume path は、選択した user task kernel stack top を
`kernel::task::architecture::install_kernel_stack(...)` まで `VirtAddr` として保持します。
この facade は typed value を registered stack-installer callback へ渡し、`main.rs` が
final TSS write の前に x86_64-owned `PrivilegeStackTopAddress` へ適合させます。

ELF loading と user stack allocation は active CR3 ではなく explicit `UserAddressSpace` へ map します。
one-shot user lifecycle は Ring 3 entry 前に task address space へ切り替え、`SYS_EXIT` 後に kernel
address space へ戻します。finished user task は private user-window page table を破棄し、user stack、
user ELF、user heap、page-table frame を reusable allocator へ返します。kernel address space へ
戻すときは、保存済み root を typed `PhysicalFrameStart` helper 経由で読み直してから CR3 に書き込みます。

## replacement checklist の読み方

英語版には実装済み/未実装の checklist が残っています。未完了項目は今後の allocator behavior
change ごとに `just storage-smoke` で boot path を継続証明すること、future guard-page reservation
owner をより精密にすること、`LOADER_DATA` reservation をさらに細かい owner に分けることです。
