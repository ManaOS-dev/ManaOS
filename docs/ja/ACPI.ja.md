# ManaOS ACPI Foundation

この文書は [`../ACPI.md`](../ACPI.md) の日本語版です。ManaOS における ACPI は、
将来の IOAPIC / Local APIC 移行のために、firmware が提供する hardware topology を
kernel が理解するための基盤です。

## 責務分担

- `main.rs` は、UEFI boot services が利用できる間に ACPI configuration table entry を
  発見します。
- `kernel::acpi` は RSDP と、選択された RSDT/XSDT root table を検証します。
- `arch/` は architecture-specific な interrupt controller programming を所有します。
  `arch/` から `kernel::acpi` へ直接 callback してはいけません。

この分担により、ACPI の解釈は kernel 側に閉じ、APIC/IOAPIC の register 操作は
architecture layer に閉じます。両者の配線は composition root である `main.rs` が行います。

## 現在検証している範囲

現在の ACPI foundation は、以下を検証または診断します。

- RSDP signature。
- RSDP ACPI 1.0 checksum。
- ACPI 2.0 extended RSDP length と checksum。
- RSDT/XSDT signature。
- RSDT/XSDT length、checksum、root entry count。
- MADT (`APIC`) table を探す RSDT/XSDT entry walk。
- MADT signature、length、checksum。
- MADT Local APIC address、raw flags、compatibility flags、interrupt-controller entry count。
- Processor Local APIC、IOAPIC、interrupt source override、Local APIC NMI、
  Local APIC address override、Processor Local x2APIC diagnostics。
- Processor Local APIC、IOAPIC、interrupt source override、Local APIC NMI、
  Processor Local x2APIC entry の bounded topology record。
- MADT interrupt source override から導出する legacy IRQ から global system interrupt への解決。
- `main.rs` が kernel-owned ACPI topology record から変換する architecture-owned APIC routing
  provider configuration。
- timer、keyboard、mouse vector 用の dry-run IOAPIC redirection entry。
- composition root からの guarded IOAPIC MMIO mapping。
- masked IOAPIC redirection entry staging と、IOAPIC version、table range、register readback diagnostics。
- composition root からの guarded Local APIC MMIO mapping。
- Local APIC EOI-provider diagnostics。
- IOAPIC routing が明示的に有効化されるまで legacy PIC を acknowledge し続ける unified EOI dispatch。
- timer、keyboard、mouse interrupt の IOAPIC route activation と readback diagnostics。
- normal APIC boot で legacy PIC が masked かつ fallback-disabled のままになることを示す diagnostics。
- IOAPIC route activation 後、timer interrupt が APIC EOI で acknowledge されることを示す counter diagnostics。
- interrupt を unmask せずに Local APIC timer を arm し、PIT tick と count delta を比較する calibration diagnostics。
- IOAPIC PIT timer route を mask し、scheduler tick を既存 vector 32 interrupt path へ維持する periodic Local APIC timer activation。
- Local APIC spurious vector 255 setup と、spurious / unexpected external vector の IDT counter。

## boot smoke で確認していること

boot smoke は以下を serial log で確認します。

- validated root table。
- MADT diagnostics。
- retained interrupt topology。
- APIC routing provider configuration。
- dry-run IOAPIC redirection plan。
- Local APIC / IOAPIC MMIO mapping。
- Local APIC EOI-provider diagnostics。
- masked IOAPIC redirection readback。
- IOAPIC route activation。
- post-activation APIC EOI count。
- legacy PIC backend が masked かつ fallback delivery disabled であること。
- masked Local APIC timer sample が decrement すること。
- IOAPIC PIT timer route が mask されること。
- periodic Local APIC timer tick が scheduler progress を進めること。
- Local APIC spurious vector が IDT diagnostic vector と一致すること。
- boot 中に spurious / unexpected external interrupt を観測しないこと。

## 次の方向

Local APIC timer によって user preemption が PIT route へ依存しなくなったため、次の大きな
作業は full process lifecycle です。`execve`、`waitpid`、minimal user shell、
general spawned process scheduling を、既存の APIC/timer foundation の上に積みます。
