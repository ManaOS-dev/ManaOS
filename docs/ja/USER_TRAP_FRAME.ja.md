# ManaOS User Trap Frame

この文書は [`../USER_TRAP_FRAME.md`](../USER_TRAP_FRAME.md) の日本語版です。ManaOS が user
task を preempt したり、任意の interrupt/syscall 後に user task を resume したりする前に必要な、
saved user register layout を定義します。

## current entry context

`kernel::task::context::UserTaskContext` は initial-entry frame です。これは
`arch/x86_64/context_switch.s` が user register を復元する前に `UserTrapFrame` へ変換されます。

復元される内容:

- `iretq` frame 用の `rip`、`cs`、`rflags`、`rsp`、`ss`。
- `rdi`、`rsi`、`rdx` に入る `argc`、`argv`、`envp`。

initial context は最初の user trap frame を作るには十分です。SYSCALL entry path は current task の
guarded kernel stack へ切り替え、runtime `UserTrapFrame` を capture し、returning syscall frame を
current user task に保存します。x86_64 timer interrupt entry も Ring 3 timer frame の complete
general-purpose register snapshot を capture し、current user task に記録します。
Scheduler task snapshot と `tasks` command は syscall / timer interrupt frame が記録済みかどうかと、
保存済み `UserTrapFrame` の byte size を公開します。storage smoke は、timer-preempted user task
snapshot すべてに recorded interrupt frame と full frame size があることを assert します。

scheduler は timer path から Ring 3 task を preempt し、別の schedulable task を実行し、saved timer
interrupt context から preempted user task を resume できます。user task は `UserAddressSpace`
root も持つため、lifecycle path は Ring 3 entry 前に CR3 を切り替え、`SYS_EXIT` 後に kernel address
space へ戻します。

## full trap frame layout

`kernel::task::context::UserTrapFrame` は user task resume contract です。field order は
`#[repr(C)]` と compile-time offset assertion で固定されています。

| Offset | Field | Source |
| --- | --- | --- |
| `0` | `instruction_pointer` | hardware `iretq` frame `rip` |
| `8` | `code_segment` | hardware `iretq` frame `cs` |
| `16` | `cpu_flags` | hardware `iretq` frame `rflags` |
| `24` | `stack_pointer` | hardware `iretq` frame `rsp` |
| `32` | `stack_segment` | hardware `iretq` frame `ss` |
| `40` | `rax` | software save |
| `48` | `rbx` | software save |
| `56` | `rcx` | software save |
| `64` | `rdx` | software save |
| `72` | `rsi` | software save |
| `80` | `rdi` | software save |
| `88` | `rbp` | software save |
| `96` | `r8` | software save |
| `104` | `r9` | software save |
| `112` | `r10` | software save |
| `120` | `r11` | software save |
| `128` | `r12` | software save |
| `136` | `r13` | software save |
| `144` | `r14` | software save |
| `152` | `r15` | software save |

total size は 160 bytes です。`rsp` は general register としては別に持たず、`iretq` frame の
user stack pointer から復元します。

## interrupt save set

user mode から入る interrupt では、architecture entry path は以下を保存します。

- hardware frame: `rip`、`cs`、`rflags`、`rsp`、`ss`。
- caller-saved registers: `rax`、`rcx`、`rdx`、`rsi`、`rdi`、`r8`、`r9`、`r10`、`r11`。
- callee-saved registers: `rbx`、`rbp`、`r12`、`r13`、`r14`、`r15`。

timer interrupt は、これらすべてを owning task の `UserTrapFrame` に capture するまで user task から
schedule away してはいけません。

timer path は assembly stub を通り、general-purpose register set を保存してから Rust architecture
hook を呼びます。architecture hook は active interrupt-controller backend を acknowledge し、
shared timer frame を `kernel::interrupt` へ渡します。これにより `arch/` が `kernel/` に依存せずに、
Ring 3 timer frame を task metadata へ記録できます。

## syscall save set

syscall path は syscall ABI で戻るために十分な state を保存します。

- architecture syscall entry が渡す user return address と flags。
- syscall entry 時点の user stack pointer。
- user code が return 後に観測し得る syscall argument register。
- すべての callee-saved register。

ManaOS は syscall と interrupt で同じ `UserTrapFrame` shape を使うべきです。scheduler が別々の
resume format を持たずに済むためです。

`kernel::interrupt::syscall_entry` は user stack から current task の guarded kernel stack へ切り替え、
この frame を作って Rust syscall dispatcher を呼びます。dispatcher は syscall result を `rax` に書き、
user selector を埋め、returning syscall frame を current user task metadata に記録します。

## preemption enablement checklist

user task preemption は、以下が満たされるまで有効化しません。

- user-mode interrupt/syscall entry が complete `UserTrapFrame` を保存する。
- user task metadata が saved trap frame を所有し、task lifecycle helper 経由でだけ公開する。
  scheduler snapshot は recorded frame flag と full saved frame byte size を公開し、smoke assertion
  で確認します。
- per-task kernel stack が存在し、user task entry/resume 前に install される。
- user task が ELF と stack mapping 用の separate page-table root を所有する。
- timer interrupt routing が user-mode frame と kernel-mode frame を区別でき、`arch/` が
  `kernel::task` に依存しない。
- page-fault diagnostics が user/kernel mode のどちらで fault したかを報告できる。
- scheduler が trap frame 保存後にだけ user task を `Running` から `Ready` へ遷移できる。
- scheduler diagnostics が task state count、user address-space ownership、preemption accounting を公開する。
- `tasks` console command が retained task ごとに lifecycle state、active membership、address-space ownership、
  scheduler-managed kernel stack ownership を表示する。
- console overlay が scheduler/preemption counters を常時見える形で表示する。
- `just storage-smoke` が one-shot user path、Local APIC timer preemption、resume、separate stack slot、
  separate address space、lifecycle diagnostics を証明する。

## 現在の到達点

現在の smoke path は、scheduler-owned exit queue、explicit return window、user stop syscall での
preemption window close、active user lifecycle drain、syscall trace control、`getpid` / `getppid`、
retained parent-child metadata、`waitpid` に向けた parent-keyed child exit record と
waitable/collected exit status model まで検証しています。
