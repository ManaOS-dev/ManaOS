# ManaOS TODO

このロードマップは未完了の作業だけを載せます。次にやることを決めやすくするため、完了済みの履歴は削除しています。

## 直近の優先事項

- [ ] `BumpFrameAllocator::allocate_frames` の複数フレーム確保時のゼロ番地スキップ挙動を修正する
- [ ] 実行不要な kernel/user mapping に `NO_EXECUTE` を設定する
- [ ] `draw_text` 呼び出しごとの font parse をやめ、parse 済み font face を cache する
- [ ] display command queue を multi-producer 前提で正しい設計に置き換える
- [ ] cursor backup の寸法に cursor size 定数を使う
- [ ] command が増えた段階で kernel console command dispatch を command 単位の module に分割する

## Phase 5: Filesystem And Storage

### Storage Driver

- [ ] AHCI probe 経路を boot-only smoke test ではなく永続的な block-device service にする
- [ ] 安定した device identifier を持つ storage device registry を追加する
- [ ] AHCI command path で multi-sector read を support する
- [ ] FAT32 cluster 境界をまたぐ read を support する
- [ ] AHCI error を `bool` だけではなく原因付きで伝搬する
- [ ] AHCI timeout diagnostics に port と command slot state を含める
- [ ] polling だけでなく AHCI interrupt-driven completion を追加する
- [ ] DMA buffer の cache invalidation または ownership rule を明文化する
- [ ] read-only storage が安定した後に AHCI sector write を追加する
- [ ] QEMU 起動と serial log 期待値確認を自動化する storage test mode を追加する

### Partition And Filesystem Parsing

- [ ] GPT header CRC を検証してから partition metadata を信用する
- [ ] GPT partition array CRC を検証する
- [ ] primary GPT header が壊れている場合に backup GPT header へ fallback する
- [ ] 常に最初の entry を選ぶのではなく、type GUID または名前で partition を選べるようにする
- [ ] FAT32 FSInfo sector metadata を parse する
- [ ] FAT32 backup boot sector を検証する
- [ ] FAT32 long file name entry を実装する
- [ ] root directory 以外の FAT32 directory traversal を実装する
- [ ] FAT32 file read を cluster chain 全体に対応させる
- [ ] FAT32 cluster chain loop と不正 cluster number を検出する
- [ ] FAT32 read-only directory listing API を実装する
- [ ] disk image を変更する前に FAT32 write 方針を設計する

### Virtual Filesystem

- [ ] mount point と filesystem backend を持つ実 mount table を追加する
- [ ] boot 時に 1 ファイルを memory にコピーするのではなく、FAT32 を filesystem backend として mount する
- [ ] directory と nested file の path traversal を追加する
- [ ] `stat` などの file metadata operation を追加する
- [ ] file descriptor に `seek` support を追加する
- [ ] directory handle と `readdir` support を追加する
- [ ] read-only / writable mount flag を追加する
- [ ] filesystem error を詳細化し、syscall errno value へ一貫して mapping する
- [ ] `/dev` directory listing を追加する
- [ ] `..`、連続 slash、末尾 slash の pathname normalization rule を決めて文書化する

### Kernel Console Commands

- [ ] command parsing と個別 command を `kernel::console::mod.rs` から分離する
- [ ] `ls` を追加する
- [ ] `pwd` を追加する
- [ ] `cd` を追加する
- [ ] `stat` を追加する
- [ ] `mounts` を追加する
- [ ] `hexdump` を追加する
- [ ] command history を追加する
- [ ] cursor movement と line editing を追加する
- [ ] console output の scrollback を追加する
- [ ] `cat /disk/hello.txt` を manual smoke test として docs に追加する

## Phase 6: Userland

### ELF And Process Loading

- [x] 64-bit ELF loader を実装する
- [x] ELF header、program header、segment permission を検証する
- [x] user text、rodata、data、bss、stack、guard page を正しい flag で map する
- [ ] `argc`、`argv`、environment pointer を user entry point に渡す
- [x] `include_bytes!` ではなく filesystem から user program を load する
- [ ] `execve` を追加する
- [ ] process identifier と parent-child relationship を追加する
- [ ] `wait` または `waitpid` を追加する
- [ ] 最小 user shell process を追加する
- [ ] `/disk/hello.txt` を open する userland test program を追加する

### Syscall Surface

- [ ] kernel と userland が共有できる syscall number / ABI contract を定義する
- [ ] `lseek` を追加する
- [ ] `stat` または `newfstatat` を追加する
- [ ] `getdents64` を追加する
- [ ] `brk` または最初の heap growth syscall を追加する
- [ ] `mmap` / `munmap` の設計を追加する
- [ ] `nanosleep` または最小 sleep syscall を追加する
- [ ] `getpid` を追加する
- [ ] `fork` を追加する、または最初の process model が `spawn`/`exec` である理由を文書化する
- [ ] syscall tracing control を追加する

### Userland Runtime

- [ ] no-std userland support crate を小さな runtime に育てる
- [ ] panic 時に明確な status で exit する処理を追加する
- [ ] userland output 用の基本 formatting helper を追加する
- [ ] userland file descriptor wrapper を追加する
- [ ] argument parsing helper を追加する
- [ ] 複数 userland binary 用 build script を追加する
- [ ] userland smoke-test runner を追加する

## Phase 7: Kernel Hardening

### Memory Management

- [ ] bump frame allocator を再利用可能な physical frame allocator に置き換える
- [ ] reserved / used / free physical frame range を追跡する
- [ ] dynamic mapping 用 kernel virtual memory allocator を追加する
- [ ] kernel stack に guard page を追加する
- [ ] process ごとの page table を追加する
- [ ] user pointer validation を一貫させる copy-in / copy-out helper を追加する
- [ ] syscall validation で writable / user / executable page permission を検証する
- [ ] identity mapping の寿命を audit し、可能なら縮小する
- [ ] raw `u64` が boundary を漏れている箇所に typed physical / virtual address wrapper を追加する
- [ ] page fault diagnostics に current task と access type を含める

### Interrupts And Scheduling

- [ ] ACPI RSDP と XSDT/RSDT を parse する
- [ ] ACPI MADT を parse する
- [ ] IOAPIC routing を有効化する
- [ ] IOAPIC 安定後に legacy PIC routing を置き換える
- [ ] Local APIC timer を calibrate して使用する
- [ ] Local APIC timer 検証後に PIT scheduling tick を置き換える
- [ ] interrupt / syscall path で完全な user trap frame を保存・復元する
- [ ] user task の preemptive scheduling を安全にする
- [ ] scheduler accounting と task state diagnostics を追加する
- [ ] 必要な箇所で task ごとの kernel stack switching を追加する

### Synchronization And Concurrency

- [ ] interrupt-time lock の deadlock / priority inversion risk を audit する
- [ ] producer/consumer 前提が合っていない queue を置き換える
- [ ] single-producer/single-consumer と multi-producer queue type を明示的に分ける
- [ ] interrupt context から呼べる API を定義する
- [ ] kernel subsystem の lock ordering note を追加する

## Phase 8: Drivers And Hardware

### Input

- [ ] keyboard layout choice を小さな configuration boundary の後ろに移す
- [ ] 必要な範囲で key release handling を追加する
- [ ] Shift / Control / Alt / Super の modifier state reporting を追加する
- [ ] Caps Lock state と LED update を追加する
- [ ] mouse wheel packet support を追加する
- [ ] double-click / drag state は input driver ではなく UI layer で追加する

### Display

- [ ] graphical overlay とは独立した scroll 対応 text console を追加する
- [ ] framebuffer mode diagnostics を追加する
- [ ] dirty rectangle の damage tracking test を追加する
- [ ] primitive window/widget layer を設計する
- [ ] UI が asset を使い始める場合に bitmap image rendering support を追加する

### Future Hardware

- [ ] AHCI read/write 安定後に NVMe support を調査する
- [ ] ACPI/interrupt work 後に USB keyboard/mouse support を調査する
- [ ] PCI capability parsing を追加する
- [ ] MSI/MSI-X の設計を追加する

## Phase 9: Tooling, Tests, And Documentation

- [ ] CI 用 headless QEMU smoke test script を追加する
- [x] boot milestone の serial log assertion を追加する
- [x] 複数 file / directory を持つ disk-image fixture generator を追加する
- [ ] byte fixture を使った GPT / FAT32 parser unit test を追加する
- [ ] success path と errno path の syscall ABI test を追加する
- [ ] commit された全 user program の userland build check を CI に追加する
- [ ] `arch` から `kernel` への import を拒否する architecture boundary check を追加する
- [ ] direct maintainer branch workflow の docs を追加する
- [ ] manual QEMU validation command の docs を追加する
- [ ] 現在の module tree から contributor 向け architecture map を生成する
