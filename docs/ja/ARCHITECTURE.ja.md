# ManaOS アーキテクチャ

## モジュールの責務ルール

各モジュールは1つの責務を持ちます。コードを追加する前に以下の問いを立ててください：
「これは既存のモジュールに属するか、それとも新しいモジュールが必要か？」

`main.rs` は構成ルートなので、アーキテクチャ固有モジュールとカーネルモジュールの両方を知ることができます。
それ以外のモジュールは、責務と依存境界を小さく明確に保ちます。

## データフロー

ハードウェア割り込み -> `arch` の割り込みハンドラ -> 登録済みコールバック -> `kernel::interrupt` bridge -> private なカーネルキューまたは scheduler -> メインループの processor -> 状態更新 -> 描画コマンド -> display driver。

重要なのは、`arch/` がイベントの配送先となるカーネルサブシステムを知らないことです。
`arch/` はハードウェア状態の読み取り、割り込みコントローラへの通知、`main.rs` が登録したコールバックの呼び出しだけを担当します。

## 依存関係のルール (厳守)

- `arch/` は絶対に `kernel/` に依存してはいけません
- `kernel/driver/` は `kernel/memory/` に依存しても構いません
- `kernel/driver/display/` は絶対に `kernel/driver/input/` に依存してはいけません
- `main.rs` はシステム全体を組み立てる唯一のモジュールです

## 割り込みの配線

`arch/x86_64/interrupt_descriptor_table.rs` の割り込みハンドラは、必ず小さく保ちます。

- 必要なハードウェアバイトまたは tick 状態を読む
- 割り込みコントローラへ通知する
- 登録済みコールバックがあれば呼び出す

現在、コールバック登録は `main.rs` で行います。

- timer tick -> `kernel::interrupt::process_timer_tick`
- keyboard byte -> `kernel::interrupt::push_keyboard_byte`
- mouse byte -> `kernel::interrupt::push_mouse_byte`

この形により、依存方向を以下のように保ちます。

```text
main.rs -> arch/
main.rs -> kernel/
arch/   -> 登録済みコールバックのみ
kernel/ -> 明示的な architecture API を除き arch 内部へ依存しない
```

architecture 側は `InterruptProcessors` 構造体と `register_processors(...)` を公開します。
その構造体は、構成ルートである `main.rs` が組み立てます。
`kernel::interrupt` は薄い bridge 関数を提供し、`main.rs` が task や input の内部へ直接配線しすぎないようにします。

timer tick の読み取りも同じ composition-root rule に従います。architecture layer が hardware tick
counter を所有し、`main.rs` が provider を `kernel::time` へ登録し、kernel subsystem は
`arch::x86_64` 内部ではなく `kernel::time` 経由で tick を読みます。

task switching と Ring 3 entry も同じ pattern です。architecture layer が assembly entry point と
user segment selector を所有し、`main.rs` が `kernel::task` へ登録し、scheduler は登録済み task
architecture provider だけを呼びます。

## 現在の実行モデル

現在の boot path は、single-core x86_64 UEFI kernel として起動し、対応する QEMU boot では
APIC-capable interrupt routing を使います。user task は separate address space、guarded
scheduler-owned kernel stack、retained metadata、syscall trace state、virtual-memory diagnostics を
所有できます。timer-driven user preemption は current smoke lifecycle で証明済みですが、general
spawned process lifecycle はまだ構築中です。

実用上のルール:

- CPU と interrupt mechanics は architecture code が所有します。
- lifecycle、scheduling、retained task metadata は kernel task code が所有します。
- frame、page-table、user mapping、kernel virtual range ownership は memory code が所有します。
- path、mount、descriptor、backend dispatch は filesystem code が所有します。
- `main.rs` はそれらの owner を配線するだけで、subsystem policy を溜め込まないようにします。

## 現在の既知の設計負債

- APIC 対応 boot では IOAPIC routing、Local APIC EOI、periodic Local APIC timer tick が有効です。PIT は calibration reference として短時間だけ初期化し、その後 IOAPIC PIT timer route を mask します。
- Ring 3 は filesystem からの ELF loading、syscall dispatch、separate user address spaces、
  guarded user task kernel stacks、syscall trace controls、smoke lifecycle の timer-context
  preemption coverage まで進んでいます。general `execve`、user-created child process、
  `waitpid`、minimal user shell はまだ process-lifecycle work として残っています。
- bootstrap と architecture-owned TSS/IST stack は、scheduler-owned task stack と同じ guarded
  stack diagnostics ではまだ表現されていません。
- cursor rendering は display 側の責務になりましたが、cursor shape はまだ単純な placeholder rectangle です。

## module owner の選び方

複数 subsystem をまたぐ behavior を追加する場合は、どの state を変更するかで owner を選びます。

- hardware register state は `arch/` または driver module。
- interrupt event routing は `kernel::interrupt`。
- task lifecycle と scheduling state は `kernel::task`。
- address-space と frame ownership は `kernel::memory`。
- path traversal と descriptor state は `kernel::filesystem`。
- command parsing と interactive text output は `kernel::console`。
- composition と provider registration だけが `main.rs`。

既存 module が state を所有していない場合は、`mod.rs` に business logic を入れず、focused sibling
module を追加します。

## 新しいドライバの追加 (チェックリスト)

- [ ] `templates/README.md` を読んでから、`templates/driver.rs.template` をコピーして開始する
- [ ] `mod.rs` にモジュールの責務に関するコメントを書く
- [ ] すべての `static` 変数は `private` にする
- [ ] 割り込みハンドラではハードウェア読み取り、通知、登録済みコールバックへの配送だけを行う
- [ ] main-loop work は `process_packets`、`process_input`、`process_events` など、
      内容が分かる `process_*` entry point に置く
