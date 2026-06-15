# ManaOS 完了済み TODO アーカイブ 日本語ガイド

このファイルは [`TODO_COMPLETED.md`](../../TODO_COMPLETED.md) の日本語ガイドです。完了済み
項目の正本は英語版です。ここでは、どの領域がすでに完了済みとして扱われているかを
把握しやすいように整理します。

## 何のためのファイルか

[`TODO.md`](../../TODO.md) は未完了項目だけを残す運用にしています。そのため、完了した履歴は
[`TODO_COMPLETED.md`](../../TODO_COMPLETED.md) に移動します。これにより、次に着手する作業を探すときに、
完了済みチェックが大量に混ざって判断しづらくなる問題を避けます。

## 完了済みとして退避済みの主な領域

### Process Lifecycle And User Program Execution

`execve` の kernel-side contract、shared syscall number、no-std userland wrapper、argv/envp copy-in、
bounded staging、filesystem path validation、image replacement 時の ownership / cleanup invariant の
文書化、unpublished image candidate の build/rollback smoke、successful image publish、old image reclaim、
no-return self-`execve` smoke、`tasks` command の current image diagnostics と replacement-state diagnostics、
open descriptor inheritance smoke、second program smoke、close-on-exec metadata、successful-`execve` close behavior、
current working directory preservation が完了済みです。`waitpid` は syscall contract、shared
number/constants、no-std userland wrapper、
selector validation、no-child `ECHILD` path、non-child negative smoke、parent-child lifecycle state
documentation、scheduler-owned child exit record model、double-reap prevention、wait lifecycle serial
assertions、zombie/reaped diagnostics、`tasks` command の per-task lifecycle output、
already-exited child の scheduler-backed `waitpid` reap、nonblocking `WNOHANG` smoke、first stable
model を `spawn + execve` とする decision、copy-on-write `fork` defer policy、kernel-internal spawn helper、
spawned process origin diagnostics、2つの concurrently spawned user program smoke、
spawn 前の argv/envp entry vector 表現、spawn path lookup failure と memory allocation failure の
errno mapping、
spawned process の current working directory inheritance、task-owned current directory を読む
no-std `getcwd` wrapper、argv/envp-capable user-visible spawn wrapper、
userland child wait smoke、nonzero child exit status smoke、blocking `waitpid(WAIT_ANY)` smoke、
spawn descriptor inheritance selection policy と snapshot diagnostic、
process-owned descriptor table migration、spawn descriptor inheritance enforcement、
parent-exit-while-child-lives smoke、
initial-process reparenting policy、exit record retention 後の finished child resource reclaim smoke、
最小 no-std `user_shell` binary の build、storage smoke disk image 登録、post-smoke 起動、
fixed-buffer stdin read、heap-free whitespace tokenization、fixed-buffer argv construction、absolute path execution、
relative path execution、`file_demo` launch smoke、missing-command not-found smoke、
`cd` / `exit` / `help` / `pwd` built-in smoke、bounded command error message smoke、
shell-loop EOF smoke、EOF 終了 smoke が完了済みです。post-shell kernel console availability smoke も
完了済みです。experimental shell command loop のための userland image linker envelope 拡張と、
keyboard-backed stdin の smoke-started user shell standard input 接続、空の keyboard stdin での
read wait/wake smoke、QEMU 上で smoke-owned experimental user shell の entry / exit path を
観察する manual validation docs も完了済みです。
user process scheduling は、
5つの active parent user process と2つの user-spawned child を
扱う storage smoke まで完了済みです。さらに、per-task の last preemption / last resume diagnostics と、preempted
process の exit 後も別の active process が継続する storage smoke、preempted user task から
spawned child を timer scheduling で初回 entry する coverage、timer-preempted user task ごとの
complete saved trap frame snapshot coverage、resumed user process の full runtime trap-frame restore
diagnostics と smoke assertion、syscall/timer trap frame の unified scheduler recording path、
resumed user task ごとの address-space root / kernel stack handoff diagnostics と storage smoke assertion、
address-space reclaim 中 task の scheduling guard と storage smoke assertion、active、finished、reclaiming
transition の impossible state assertion と storage smoke verification、active、waiting、zombie、reaped task の
scheduler invariant documentation も完了済みです。Phase 3 の scheduler state machine では、
storage smoke で走る scheduler invariant check が完了済みです。
正本は英語版の
`TODO_COMPLETED.md` と [`PROCESS_LIFECYCLE.md`](../PROCESS_LIFECYCLE.md) です。

### Immediate Priorities

初期の優先タスクとして、NX permission、font face cache、display command queue、
cursor backup size、console command dispatch 分割が完了済みです。

### Filesystem And Storage

AHCI persistent service、stable device registry、multi-sector read、FAT32 cluster
boundary read、AHCI error propagation、interrupt-driven completion、DMA ownership、
write support planning、QEMU storage smoke が完了済みです。

GPT/FAT32 では backup GPT fallback、partition selection、FAT32 backup boot sector、
long file name、nested directory traversal、full cluster-chain read、loop detection、
read-only listing、write planning が完了済みです。

VFS では mount table、FAT32 backend mount、path traversal、metadata、seek、
directory handle、read-only/writable mount flags、errno mapping、`/dev` listing、
pathname normalization が完了済みです。

### Userland

64-bit ELF loader、ELF validation、user segment mapping、argc/argv/envp setup、
filesystem-based user program loading、PID/parent-child metadata、file demo smoke が
完了済みです。

syscall surface では shared ABI、`lseek`、`stat`、`getdents64`、`brk`、anonymous
`mmap`/`munmap`、partial `munmap`、fixed mapping、file-private mapping、replacement
`MAP_FIXED`、`nanosleep`、`getcwd`、syscall tracing controls が完了済みです。

userland runtime では panic exit、fd wrapper、argument parser、fixed-buffer command
module、multi-binary build、smoke runner が完了済みです。

### Kernel Hardening

physical frame allocator、owner diagnostics、dynamic kernel virtual mapping、
per-process page table、user address-space reclaim、kernel stack reclaim、
user pointer validation、mapping permission checks、identity mapping audit、
page fault diagnostics が大きく進んでいます。

interrupt/scheduling では ACPI/MADT、IOAPIC、Local APIC timer、legacy PIC fallback、
spurious vector diagnostics、trap frame layout、timer preemption、scheduler diagnostics、
`tasks` command、per-task VM snapshots、scheduler-owned lifecycle drain が完了済みです。

context switch/task refactoring では context responsibility split、ABI docs、
trap-frame offset verification、process lifecycle module、scheduler-owned exit queue、
return-window invariant、preemption state normalization、task metadata model が完了済みです。

### Tooling, Tests, And Documentation

serial log assertions、disk-image fixture generator、syscall ABI tests、
architecture boundary check、direct maintainer workflow docs が完了済みです。

## 更新ルール

新しい完了項目を追加する場合は、英語版 [`TODO_COMPLETED.md`](../../TODO_COMPLETED.md) に具体的な項目を移し、
この日本語ガイドには領域単位の説明を追記してください。細かいチェックリストの正本は
英語版に集約し、日本語版は履歴の理解と導線に寄せます。
