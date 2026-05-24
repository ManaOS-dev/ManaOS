# ManaOS TODO

## Now (Current Sprint)
- [x] タイマー EOI を常に送信するよう修正（`timer_interrupt_handler` の try-lock バグ）
- [x] マウスカーソル座標を `draw_cursor()` だけでなく `state.rs` 側でクランプ
- [x] `runtime::tick()` の FPS 計算でゼロ除算を防止

## Phase 5: Filesystem & Storage
- [x] Phase 5A: kernel-side file abstraction
- [x] VFS 抽象化レイヤー
- [x] ramfs
- [x] `/dev/console`
- [x] `/dev/null`
- [x] FileDescriptor table
- [ ] Phase 5B: Userland I/O
- [x] Phase 5B-1: SYS_WRITE only
- [x] syscall ABI
- [x] SYS_WRITE 実装
- [x] user pointer validation の仮実装
- [x] userland write wrapper
- [x] `hello from userland` 出力
- [x] Phase 5B-2: SYS_EXIT plus one-shot user demo
- [x] SYS_EXIT 実装
- [x] exit 時に current user task を finished にする
- [x] one-shot user demo runner
- [x] user exit 後に UI を再開
- [x] Phase 5B-3: file syscalls
- [x] syscall open/read/close
- [x] user から C string をコピー
- [x] read buffer を user へコピー
- [x] userland open/read/close demo
- [x] Phase 5B-4: user pointer のページテーブル検証
- [x] user range を mapped page 単位で検証
- [x] 未マップの read/write syscall buffer を拒否
- [x] Phase 5B-4b: bad pointer demo mode
- [x] user demo mode を定数 1 個で切り替え
- [x] 不正な read buffer が `ERROR_BAD_ADDRESS` を返すことを検証
- [x] Phase 5B-5: userland syscall wrapper クレート
- [x] no_std userland クレート
- [x] write/read/open/close/exit の syscall wrapper
- [x] Rust file demo flat binary を kernel から include
- [ ] 最小 shell 風 task
- [ ] Phase 5C: Real Storage
- [x] Phase 5C-1: PCIe 列挙と AHCI コントローラ発見
- [x] legacy PCI config-space access
- [x] AHCI BAR5 discovery
- [x] AHCI implemented port と SATA signature のログ出力
- [x] Phase 5C-2: AHCI LBA0 read smoke test
- [x] AHCI command list/FIS/command table setup
- [x] READ DMA EXT による LBA0 読み出し
- [x] LBA0 先頭 16 bytes の serial dump
- [x] Phase 5C-3: GPT header inspection
- [x] AHCI の任意 LBA 読み出し
- [x] LBA1 GPT signature と header field のログ出力
- [x] Phase 5C-3b: GPT test image script
- [x] protective MBR と primary/backup GPT header を生成
- [x] Phase 5C-4a: 空 GPT partition entry scan
- [x] GPT partition entry array sector を読み出し
- [x] partition entry がない GPT を empty として報告
- [x] Phase 5C-4b: GPT test partition detection
- [x] disk image script で GPT partition entry を 1 つ生成
- [x] non-empty GPT partition entry count を報告
- [ ] GPT partition entry parsing
- [ ] AHCI ドライバー実装
- [ ] FAT32 パーサーとファイル API

## Phase 6: Userland
- [ ] ELF ローダー
- [ ] システムコール API 定義
- [ ] シェル実装
- [ ] 動的リンカースタブ

## Phase 7: Kernel Hardening
- [ ] ACPI MADT 解析
- [ ] IOAPIC ルーティング（legacy 8259 PIC の置き換え）
- [ ] Local APIC timer（PIT の置き換え）
- [ ] コンテキストスイッチ時に完全な user trap frame を保存/復元
- [ ] プロセスごとの仮想アドレス空間（ページテーブル分離）
- [ ] カーネルスタック間のガードページ
- [ ] 仮想メモリアロケーター（動的カーネルマッピング用）
- [ ] スクロール対応のコンソールテキスト出力
- [ ] ウィンドウ / ウィジェットのプリミティブレイヤー

## Completed
<details>
<summary>Phase 1-4（クリックして展開）</summary>

### リファクタリング

- [x] boot 時の memory/display 初期化を `main.rs` から分離
- [x] メインループの tick 処理を `main.rs` から分離
- [x] interrupt handler から `arch/` -> `kernel/` の直接呼び出しを削除
- [x] `main.rs` から interrupt callback を配線
- [x] interrupt callback 登録を単一の `InterruptProcessors` API に整理
- [x] kernel 側 interrupt event routing 用の `kernel::interrupt` bridge を追加
- [x] boot-service pool allocation 後に stale boot memory map を使う問題を修正
- [x] `process_packets()` 呼び出し間で PS/2 mouse packet assembly state を保持
- [x] framebuffer lock 競合時に display command processing が command を落とさないよう修正
- [x] unsafe-heavy module に不足していた `// SAFETY:` コメントを追加
- [x] cursor rendering の責務を input mouse code から display cursor code へ移動

### Phase 1: Memory Management & Foundation

- [x] Memory Map Acquisition & `ExitBootServices`
- [x] Physical Frame Allocator（Bump Allocator）
- [x] Heap Allocator（`linked_list_allocator`）
- [x] Architecture Separation（`arch/` layer established）
- [x] Explicit Paging Setup（Identity Mapping）
- [x] boot-service allocation 後の最終 memory map から allocator region を再構築/更新

### Phase 2: Interrupts & Exceptions

- [x] GDT / IDT Setup（with Data Segments）
- [x] Exception Handlers（Page Fault, Double Fault, GPF）
- [x] Mouse Driver（PS/2）with Real-time Cursor, Lock-Free Async Queue & Dirty Rectangles
- [x] Keyboard Driver（PS/2）- Interrupt driven & Lock-Free Async Queue
- [x] Interrupt callback boundary: `arch/` は `kernel/` ではなく登録 callback へ dispatch
- [x] callback registration を `InterruptProcessors` に統合
- [x] PS/2 controller busy wait に timeout を追加
- [x] Local APIC capability detection 付き timer backend abstraction
- [x] IOAPIC routing boundary 付き interrupt controller abstraction

### Phase 3: Graphics & Console

- [x] Serial Output（COM1）
- [x] GOP Framebuffer Control
- [x] Font Engine（`ab_glyph`）
- [x] Proper Alpha Blending for Text（Pixel-perfect rounding）
- [x] Double Buffering & Dirty Rectangles Optimization（1000fps ready）
- [x] RDTSC Profiling & Calibration
- [x] `framebuffer.rs` から renderer/font/cursor responsibility を分離
- [x] 一時的な framebuffer lock contention で queued draw command を落とさないよう修正

### Phase 4: Process Management

- [x] Task Structure & Context Switching
- [x] Cooperative / Preemptive Scheduler
- [x] Ring 3 descriptor groundwork and selector exposure
- [x] `iretq` と user stack による user mode への遷移
- [x] 最小 `SYSCALL`/`SYSRET` MSR setup と syscall bridge stub

</details>
