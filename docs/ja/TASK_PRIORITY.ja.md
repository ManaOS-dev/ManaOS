# ManaOS Task Priority

この文書は [`../TASK_PRIORITY.md`](../TASK_PRIORITY.md) の日本語版です。現在の未完了作業を、
実装難易度と dependency depth の観点で並べます。product value の優先順位ではなく、
kernel engineering 上の順序を示す文書です。

## 残っている高リスク作業の順序

1. Full user process lifecycle
   - `execve` の replacement-state diagnostics の残り gap を完了する。
   - user-visible `wait` または `waitpid` を追加する。
   - minimal user shell process を追加する。
   - general process lifecycle path へ preemptive scheduling を拡張する。
   - 理由: ELF loading、syscall ABI、address-space ownership、file descriptor、
     parent-child metadata、scheduler cleanup をまたぐため。

2. Remaining per-task kernel stack completion
   - bootstrap と architecture-owned TSS/IST stack を diagnostics に表現する。
   - non-scheduler stack の guard-page ownership を完了する。
   - 理由: fault handling と low-level stack safety に触るが、scheduler-owned stack path は
     すでに入っているため。

3. Typed physical and virtual address wrapper sweep
   - subsystem boundary を越える raw `u64` address leakage を置き換える。
   - 理由: call site は広いが、既存 newtype と test の後ろで段階的に進められるため。

4. Synchronization and interrupt-time lock audit
   - interrupt-callable API を定義する。
   - lock ordering note を追加する。
   - producer/consumer assumption がずれている queue を分ける。
   - 理由: SMP や APIC path が増えるまでは多くが diagnostic/structural work だが、
     後回しにすると原因調査が難しくなるため。

5. Storage mutation and parser test expansion
   - GPT / FAT32 parser fixture を追加する。
   - FAT32 create、grow、truncate、unlink、directory operation の mutation semantics を追加する。
   - storage reliability counter、retry policy、write smoke coverage を追加する。
   - 理由: read path が安定しているため、disk image を変更する write path に進む前に test で保護するため。

6. Input/display/userland quality work
   - keyboard layout boundary、key release、modifier state、text console、damage tracking、
     formatting helper、CI build check。
   - 理由: 重要ではあるが、kernel execution model の blocking issue ではないため。

## 現在の選択

ACPI/APIC interrupt migration は active selection ではなくなりました。ACPI root discovery、
RSDT/XSDT validation、MADT validation、APIC routing provider configuration、IOAPIC route
activation、legacy PIC fallback masking、Local APIC timer calibration、periodic Local APIC
scheduler tick、spurious/unexpected external vector diagnostics は storage smoke で証明済みです。

そのため次の大きな流れは、PIT route に依存しなくなった timer preemption の上で、
full user process lifecycle を進めることです。`execve` の kernel-side contract、cleanup invariant、
successful self-replacement path、current directory preservation、path-only `spawn`、nonblocking
`waitpid(WNOHANG)` child collection smoke と nonzero child status encoding は
[`PROCESS_LIFECYCLE.ja.md`](PROCESS_LIFECYCLE.ja.md) に整理済みです。ここからは小さい runtime slice で進めます。

1. `execve` の replacement-state diagnostics の残り gap を閉じる。
2. waiting parent を sleep/wake できるようになったら blocking `waitpid` behavior を追加する。
3. user-visible spawn を path-only launch から argv/envp vector 付きへ拡張する。
4. lifecycle state に新しい transition が増えたら scheduler diagnostics も更新する。

広い syscall surface を一気に増やす前に、docs、diagnostics、narrow smoke assertion を優先します。
