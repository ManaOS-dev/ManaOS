# ManaOS Task Priority

この文書は [`../TASK_PRIORITY.md`](../TASK_PRIORITY.md) の日本語版です。現在の未完了作業を、
実装難易度と dependency depth の観点で並べます。product value の優先順位ではなく、
kernel engineering 上の順序を示す文書です。

## 難しいものから見た順序

1. ACPI と APIC interrupt migration
   - ACPI RSDP と RSDT/XSDT を parse する。
   - MADT entry を parse する。
   - IOAPIC routing を有効化する。
   - legacy PIC routing を置き換える。
   - legacy PIC fallback boundary を harden する。
   - masked Local APIC timer calibration diagnostics を追加する。
   - scheduling tick を Local APIC timer へ切り替える。
   - spurious / unexpected external interrupt vector diagnostics を追加する。
   - 理由: early boot discovery、interrupt topology、timer ownership、
     architecture/kernel wiring を同時に変えるため。

2. Full user process lifecycle
   - `execve` を追加する。
   - user-visible `wait` または `waitpid` を追加する。
   - minimal user shell process を追加する。
   - general process lifecycle path へ preemptive scheduling を拡張する。
   - 理由: ELF loading、syscall ABI、address-space ownership、file descriptor、
     parent-child metadata、scheduler cleanup をまたぐため。

3. Remaining per-task kernel stack completion
   - bootstrap と architecture-owned TSS/IST stack を diagnostics に表現する。
   - non-scheduler stack の guard-page ownership を完了する。
   - 理由: fault handling と low-level stack safety に触るが、scheduler-owned stack path は
     すでに入っているため。

4. Typed physical and virtual address wrapper sweep
   - subsystem boundary を越える raw `u64` address leakage を置き換える。
   - 理由: call site は広いが、既存 newtype と test の後ろで段階的に進められるため。

5. Synchronization and interrupt-time lock audit
   - interrupt-callable API を定義する。
   - lock ordering note を追加する。
   - producer/consumer assumption がずれている queue を分ける。
   - 理由: SMP や APIC path が増えるまでは多くが diagnostic/structural work だが、
     後回しにすると原因調査が難しくなるため。

6. Input/display/userland quality work
   - keyboard layout boundary、key release、modifier state、text console、damage tracking、
     formatting helper、CI build check。
   - 理由: 重要ではあるが、kernel execution model の blocking issue ではないため。

## 現在の選択

ACPI/APIC interrupt migration は、root discovery、RSDT/XSDT validation、MADT validation、
bounded topology diagnostics、APIC routing provider configuration、dry-run IOAPIC redirection、
masked IOAPIC MMIO staging、Local APIC EOI diagnostics、unified EOI dispatch、active IOAPIC
routing、APIC EOI counter、legacy PIC fallback masking、masked Local APIC timer calibration、
periodic Local APIC scheduler tick、spurious/unexpected vector diagnostics まで storage smoke で
証明済みです。

そのため次の大きな流れは、PIT route に依存しなくなった timer preemption の上で、
full user process lifecycle を進めることです。具体的には `execve`、`waitpid`、minimal user
shell、general spawned process scheduling が中心になります。
