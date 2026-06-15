# ManaOS TODO 日本語ガイド

このファイルは [`TODO.md`](../../TODO.md) の日本語ガイドです。実装対象としての正本は
英語版の `TODO.md` です。日本語側では、フェーズの意図、読み方、着手順の判断材料を
詳しく説明します。

完了済み項目は [`TODO_COMPLETED.md`](../../TODO_COMPLETED.md) に退避されています。
今後 TODO を完了にする場合は、実装・検証・コミット後に英語版 `TODO.md` から削除し、
完了済みアーカイブへ移動してください。

## 運用ルール

- [`TODO.md`](../../TODO.md) には未完了項目だけを残します。
- 完了済み項目は [`TODO_COMPLETED.md`](../../TODO_COMPLETED.md) に移動します。
- 日本語の会話は歓迎されますが、コード、コメント、コミットメッセージは英語です。
- 1つの TODO は、できるだけ1つのレビュー可能なブランチに収まる大きさへ分割します。
- kernel、userland、architecture、syscall、memory、scheduler の境界を触る場合は、
  実装前に関連 docs と `AGENTS.md` の境界ルールを読みます。
- docs-only の変更でも、`git diff --check` または `git show --check` で Markdown の
  空白エラーを確認します。

## Phase 1: Process Lifecycle And User Program Execution

このフェーズは、ManaOS の user process を「起動できる smoke demo」から
「親子関係、exec、wait、shell を持つ実用的なプロセスモデル」へ進める作業です。

主な焦点:

- `execve` による user image replacement。
- `waitpid` と exit status の保持、reap、zombie 管理。
- `spawn + execve` を最初の安定モデルとして進めるための残り実装。
- 最小 user shell の導入。
- address space reclaim 中の task を schedule しない scheduler state 保護。

実装時の注意:

- `execve` は syscall ABI、filesystem、ELF loader、address-space cleanup、
  file descriptor inheritance、scheduler metadata をまたぎます。
- 失敗時の rollback が重要です。途中で作った address space、stack、page table、
  argv/envp buffer を漏らしてはいけません。
- `waitpid` は exit status を保持するだけでなく、いつ資源を返すかを明確にする必要が
  あります。
- shell は最初から豪華にせず、fixed buffer と no-std runtime の制約を守ることを
  優先します。

`execve` の kernel-side contract、shared syscall number、no-std userland wrapper、argv/envp copy-in、
bounded staging、filesystem path validation、cleanup invariant、successful image publish、old image
reclaim、no-return self-`execve` smoke、open descriptor inheritance smoke、second program smoke、
successful `execve` across current working directory preservation は
[`PROCESS_LIFECYCLE.ja.md`](PROCESS_LIFECYCLE.ja.md) に整理済みです。`waitpid` は syscall ABI
contract、shared number/constants、no-std wrapper、selector validation、no-child `ECHILD`
path、parent-child lifecycle state documentation、scheduler-owned child exit record model、
zombie/reaped diagnostics、`tasks` command の per-task lifecycle output、already-exited child の
scheduler-backed `waitpid` reap、nonblocking `WNOHANG` smoke、filesystem path から user task を
作る kernel-internal spawn helper と spawned process origin diagnostics、2つの concurrently spawned
user program smoke、spawn 前の argv/envp entry vector 表現、spawn path lookup failure と memory
allocation failure の errno mapping、argv/envp-capable user-visible spawn wrapper、
userland child wait smoke、nonzero child exit status smoke、blocking `waitpid(WAIT_ANY)` smoke まで
完了済みです。
spawned process の current working directory inheritance も完了済みです。
最初の stable process model は `spawn + execve` として決定済みで、minimal `fork` は Phase 2 の
address-space copy plan ができるまで defer します。
最小 no-std `user_shell` binary は userland build と storage smoke disk image に入り、storage smoke 後に
実行されます。fixed-buffer stdin read、heap-free whitespace tokenization、fixed-buffer argv construction、
absolute path execution、relative path execution、`file_demo` launch smoke、missing-command not-found smoke、
`cd` / `exit` / `help` / `pwd` built-in smoke、bounded command error message smoke、shell-loop EOF smoke、
EOF 終了 smoke、keyboard-backed stdin の smoke-started user shell standard input 接続、
keyboard-backed stdin が空の間の read wait/wake smoke は完了済みです。
post-shell kernel console availability smoke も完了済みです。QEMU 上で smoke-owned
experimental user shell の entry / exit path を観察する手順も manual validation docs に整理済みです。
user process scheduling は、5つの active parent user process と2つの user-spawned child を扱う storage smoke
まで完了済みです。
per-task の last preemption / last resume diagnostics と、preempted process の exit 後も別の
active process が継続する storage smoke も完了済みです。
`execve` は current working directory preservation と close-on-exec behavior まで完了済みなので、
no-std userland から current working directory を読む `getcwd` wrapper も入っています。
replacement-state diagnostics も `tasks` command に入っています。
spawn descriptor inheritance selection は process-owned descriptor table で enforcement され、storage smoke で
process-table snapshot diagnostic を確認します。parent-exit-while-child-lives smoke は initial-process
reparenting policy まで確認します。
finished child resource reclamation policy は、exit record retention 後に runtime resource を reclaim しても
waitable exit が残ることまで確認します。resumed user process の full runtime trap-frame restore と
syscall/timer return frame の unified scheduler recording path は scheduler diagnostics と storage smoke
で確認済みです。さらに address-space/kernel-stack resume handoff 証明も scheduler diagnostics と
storage smoke で確認済みです。残りは reclaim 中 task の scheduling prevention と
preemption/scheduler diagnostics です。
次も小さい branch に分けて進めます。

## Phase 2: Memory Safety, Address Spaces, And Stack Hardening

このフェーズは、物理/仮想アドレス、user mapping、kernel stack、`mmap` 周辺の
安全性を高める作業です。

主な焦点:

- raw `u64` address が module boundary を越える箇所を typed wrapper へ移すこと。
- bootstrap、TSS、IST stack まで含めた guard page 設計の完了。
- address-space の build、publish、rollback、reclaim の状態を明確にすること。
- user text、rodata、heap、stack、private mapping などの permission policy を
  名前付きで整理すること。
- `brk` と `mmap` の上限、失敗時 errno、stress smoke を追加すること。

実装時の注意:

- 物理アドレスと仮想アドレスを混ぜないことが最優先です。
- guard page は mapped page ではなく、fault させるための unmapped reservation です。
- `execve` の successful publish は実装済みです。次は allocation failure でも panic しない
  fallible construction と、post-candidate failure cleanup を固めます。
- writable executable user mapping は、明示的に許す設計ができるまで拒否する方針を
  維持します。

## Phase 3: Synchronization, Interrupt Context, And Scheduler Robustness

このフェーズは、interrupt context、lock ordering、queue primitive、scheduler state
machine、syscall/trap の堅牢性を固める作業です。

主な焦点:

- interrupt、exception、syscall の各 context から到達する lock の棚卸し。
- single-producer / multi-producer など queue の前提を型と API に反映すること。
- scheduler state transition を1つの明示的な state machine に寄せること。
- syscall entry/return と trap diagnostics を register/flags/canonical address の
  観点から audit すること。
- preemption disable scope、timer accounting、fairness smoke を追加すること。

実装時の注意:

- interrupt handler は最小限の処理だけを行い、重い処理は main loop や scheduler 側へ
  渡します。
- interrupt context で ordinary lock、allocation、長い logging を行う場合は、
  既存コードが安全性を証明している場合に限ります。
- scheduler の状態は「動けばよい」ではなく、診断で説明できる形にします。

## Phase 4: Filesystem, Storage, And Device I/O Expansion

このフェーズは、read-only で安定してきた storage/VFS を、fixture test、mutation、
reliability、device discovery、file descriptor surface へ広げる作業です。

主な焦点:

- GPT/FAT32 parser の byte fixture test。
- FAT32 の file creation、growth、truncate、unlink、directory mutation。
- AHCI retry、timeout、reset、I/O counters、request queueing。
- PCI capability、MSI/MSI-X、NVMe、USB の調査と計画。
- `openat`、`mkdir`、`unlink`、`rename`、`ftruncate`、descriptor duplication などの
  syscall surface 計画。

実装時の注意:

- FAT32 write は corruption model を先に文書化してから進めます。
- block-device error を filesystem error と syscall errno へどう写すかを明確にします。
- device discovery は、最初から hotplug まで実装せず、名前付けと診断の安定性を優先します。

## Phase 5: Drivers, Display, Input, And Console UX

このフェーズは、keyboard/mouse/display/console/UI layer の体験と診断性を高める作業です。

主な焦点:

- keyboard layout boundary、key release、modifier、lock LED。
- mouse wheel、packet resync、overflow diagnostics、UI layer での double-click/drag。
- graphical overlay と独立した text console。
- dirty rectangle test、renderer diagnostics、panic-safe text rendering。
- process、fd、mount、interrupt、queue、timer などの診断 console command。
- primitive window/widget layer の設計。

実装時の注意:

- input driver は raw input を扱い、UI semantics は UI layer に置きます。
- display driver は input に依存してはいけません。
- panic path や interrupt path で使う rendering/logging は、lock と allocation の危険を
  先に確認します。

## Phase 6: Tooling, CI, Tests, And Documentation

このフェーズは、開発者が同じ品質で build、lint、QEMU smoke、docs を扱えるようにする作業です。

主な焦点:

- headless QEMU smoke の CI 化。
- kernel/userland の check、clippy、fmt、architecture boundary check の CI 化。
- QEMU serial log artifact の保存。
- manual QEMU validation docs の更新。
- architecture map、module ownership table、lifecycle diagram の整備。
- parser、ABI、command tokenizer、invalid pointer、resource exhaustion などの test 拡張。
- release checklist、issue template、TODO pruning policy。

実装時の注意:

- CI は「全部やる」より、失敗時に原因が追える artifact を残すことを優先します。
- docs は英語正本と日本語版の対応が崩れないよう、README の文書一覧も更新します。

## Phase 7: Long-Term Platform, Security, And Multi-Architecture Foundation

このフェーズは、長期的な platform 化、安全性、SMP、networking、multi-architecture を扱います。

主な焦点:

- address exposure、stack canary、fuzzing、permission audit、threat model。
- single-core 前提の明文化と、将来の SMP blocker の整理。
- CPU topology、AP startup、per-CPU scheduler/interrupt stack、TLB shootdown。
- first NIC model、packet buffer ownership、ARP/IPv4/UDP/TCP planning。
- `x86_64` 前提を `src/arch` 外へ漏らさないための architecture provider 整理。
- bootable image packaging、version banner、support log、milestone map。

実装時の注意:

- 長期計画でも、現在の x86_64 UEFI kernel と QEMU/OVMF boot path を壊さないことが
  前提です。
- multi-architecture は抽象化だけを増やすのではなく、実際に漏れている前提を audit して
  必要な境界から切ります。

## 着手順の目安

次に着手する候補は、基本的に [`docs/TASK_PRIORITY.md`](../TASK_PRIORITY.md) と
[`TODO.md`](../../TODO.md) の Phase 1 から
選びます。大きい項目は、そのまま実装せず、先に「docs」「diagnostics」「narrow smoke」
のような小さい単位に分割してください。

特に優先度が高い流れ:

1. address space reclaim 中の task が schedule されないようにする。
2. active、finished、reclaiming transition の impossible state に scheduler assertion を追加する。
3. lifecycle state が増えたら scheduler diagnostics も同期する。
