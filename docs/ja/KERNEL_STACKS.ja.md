# ManaOS Kernel Stacks

この文書は [`../KERNEL_STACKS.md`](../KERNEL_STACKS.md) の日本語版です。kernel stack の
guard page design と、task ごとの kernel stack switching policy を定義します。

## 現在の stack model

ManaOS には現在4種類の kernel stack があります。

- Bootstrap stack: UEFI から kernel に入った時点で active な stack。現在は
  `kernel::task` の metadata では表現されていません。
- Kernel task stacks: `kernel::task::stack::KernelStack` metadata が所有します。
  higher-half writable stack mapping と、その下にある unmapped guard page で構成されます。
- User task kernel stacks: Ring 3 entry 前に `KernelStack` metadata が所有します。mapped writable
  stack range と unmapped guard page を持ち、one-shot user path は x86_64 TSS に stack top を
  install してから Ring 3 に入ります。
- Architecture stacks: `arch::x86_64::global_descriptor_table` が TSS 内の Ring 0 privilege stack と
  double-fault IST entry を所有します。現状は static byte array です。

現在の cooperative kernel task では boot-safe ですが、user preemption、nested trap、
stack overflow diagnostics の最終形としては不十分です。

## guard page placement

kernel stack は downward growth です。所有される kernel stack allocation は、writable stack
page の下に unmapped guard page を1ページ予約します。

```text
high address
  writable stack pages
  ...
  writable stack page 0
  unmapped guard page
low address
```

必須ルール:

- guard page は mapped しません。physical frame を消費しない virtual reservation として扱います。
- writable stack page は `PRESENT | WRITABLE | NO_EXECUTE` で、`USER_ACCESSIBLE` ではありません。
- stack top は最高位 mapped stack byte の1バイト先を指し、context entry 前に 16-byte aligned です。
- guard page は writable stack range に直接隣接し、downward overflow が別の kernel object を
  壊す前に fault します。
- double-fault IST stack は独立した guard page を持ち、normal task stack と page を共有しません。

## allocation policy

scheduler-owned kernel/user task stack は higher-half virtual range を予約し、lowest page を
guard page として unmapped のまま残し、writable page に physical frame を割り当てます。
mapping は kernel-only、writable、non-executable です。

finished user task は `SYS_EXIT` 後、scheduler が task を `Finished` として扱える状態になってから
stack resource を reclaim します。diagnostics は reclaimed stack count、writable page count、
guard-inclusive virtual page count を集計し、console overlay に表示します。

bootstrap stack と architecture-owned TSS/IST stack は、まだこの allocation path に統合されて
いません。

## per-task kernel stack switching policy

user task preemption を安全にするには、user task ごとに bootstrap stack とは別の kernel stack が
必要です。

policy:

- すべての schedulable task は kernel stack owner record を持ちます。
- kernel task は自分の `TaskContext` の kernel stack で entry します。
  `TaskContext::from_stack(...)` は選択済み stack top を `VirtAddr` として受け取り、
  private assembly-facing context record を埋める境界でだけ raw value へ下ろします。
- user task は syscall / interrupt handling 用の kernel stack を持ちます。
- user task は ELF / stack mapping 用の separate address space を所有します。
- user task に入る前、または resume する前に、scheduler はその task の kernel stack top を
  architecture task provider へ install し、CR3 を task の user address space へ切り替えます。
  scheduler はこの handoff を retained task snapshot に記録し、smoke は各 finished user task の
  nonzero resume handoff、address-space root、kernel stack top を確認します。
- scheduler は user-entry / timer-resume handoff path の中では、選択した kernel stack top を
  `VirtAddr` として保持します。task architecture facade と registered architecture stack-installer
  callback も `VirtAddr` として受け取り、`main.rs` が final TSS write の前に
  x86_64-owned `PrivilegeStackTopAddress` へ適合させます。別経路の `SYSCALL` entry
  stack top atomic は ABI-facing raw lowering boundary のままです。
- scheduler-owned stack guard / writable start は `KernelPageStart` として保持し、
  diagnostics が表示用に下ろす前に 4 KiB alignment を metadata 上で表現します。
- x86_64 では、TSS `privilege_stack_table[0]` を architecture-owned API 経由で更新します。
- Ring 3 interrupt entry は install 済み TSS privilege stack を使うため、timer interrupt は
  current task の guarded kernel stack に入ります。
- `SYSCALL` は TSS privilege stack を自動使用しないため、entry path が current task の stack top を
  使って user stack から guarded kernel stack へ切り替えます。
- `arch/` は `kernel::task` を直接呼びません。provider registration は `main.rs` が所有します。
- user preemption が有効な間、timer interrupt が stale bootstrap context へ戻ってはいけません。

## fault diagnostics

page fault diagnostics は、generic page-fault reporting より前に kernel stack guard fault を分類します。

- fault address が既知の guard page 内: task id、stack owner、guard page address、可能なら current
  stack pointer、fault が kernel/user mode のどちらかを報告します。
- fault address が writable stack page 内: stack pointer が writable range 外にある場合に stack
  overflow suspicion を報告します。
- fault address が既知 stack 外: generic page-fault diagnostics へ fallback します。

double-fault handling は最小限に保ちます。IST stack を使用したことを報告し、faulting path が保持して
いる可能性のある lock を取りに行かないようにします。

## implementation order

1. stack mapping 用 kernel virtual address range allocation を追加する。
2. scheduling 変更なしで `kernel::task::stack` metadata を導入する。
3. heap-backed `KernelStack` から guarded mapped stack へ移す。
4. x86_64 TSS Ring 0 stack 更新用 architecture provider を追加する。
5. user task に kernel stack を持たせ、Ring 3 entry 前に install する。
6. known guard page を検出する page-fault diagnostics を追加する。
7. full user trap frame と per-task kernel stack switching が検証されるまで user preemption を有効化しない。
8. user address space を task record に接続し、Ring 3 entry / timer-context resume 前に CR3 を切り替える。
   one-shot と timer-resume smoke path では、resume handoff diagnostics まで完了済みです。
9. `SYS_EXIT` 後に finished user task kernel stack を reclaim する。
10. reclaim accounting を scheduler diagnostics と console overlay へ公開する。
