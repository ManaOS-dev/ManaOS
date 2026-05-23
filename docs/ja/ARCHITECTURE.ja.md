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

## 現在の既知の設計負債

- boot-service のファイル割り当て前に memory region を収集しているため、最終 memory map から allocator region を更新する必要があります。
- PS/2 mouse packet の組み立て状態は `process_packets()` 呼び出しをまたいで保持する必要があります。
- display command は graphics lock が一時的に取れない場合でも失われないようにする必要があります。
- cursor rendering は `kernel::driver::input::mouse` から display cursor module へ移すべきです。
- unsafe が多いモジュールでは、`// SAFETY:` コメントと unsafe block の粒度をさらに整える必要があります。

## 新しいドライバの追加 (チェックリスト)

- [ ] `templates/driver.rs.template` をコピーして開始する
- [ ] `mod.rs` にモジュールの責務に関するコメントを書く
- [ ] すべての `static` 変数は `private` にする
- [ ] 割り込みハンドラではハードウェア読み取り、通知、登録済みコールバックへの配送だけを行う
- [ ] すべての処理はメインループから呼ばれる `process()` に記述する
