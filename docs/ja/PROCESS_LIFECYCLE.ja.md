# Process Lifecycle

この文書は [`../PROCESS_LIFECYCLE.md`](../PROCESS_LIFECYCLE.md) の日本語版です。英語版を
正本とし、ここでは ManaOS の process lifecycle、特に今後の `execve` 実装で守る境界を
説明します。

## ownership

- `kernel::syscall` は syscall number dispatch、argument register decode、
  errno 形式の result mapping、syscall tracing を所有します。
- `kernel::memory::user_pointer` は、lifecycle state を変更する前に user pointer から
  kernel-owned staging data へコピーする責務を持ちます。
- `kernel::filesystem` は path normalization、namespace lookup、file metadata、
  descriptor table operation、filesystem error value を所有します。
- `kernel::elf` は user image の ELF validation と segment mapping policy を所有します。
- `kernel::memory` は user address-space の construction、publication、rollback、
  frame reclamation を所有します。
- `kernel::task` は process identifier、parent-child metadata、scheduler state、
  trap frame、process-owned descriptor table instance、exit record、lifecycle diagnostics を所有します。
- `main.rs` は boot-time smoke wiring の composition root です。process replacement policy の
  owner にはしません。

## current status

現在の kernel は、filesystem から user ELF image を読み込み、初期 `argc` / `argv` / `envp`
stack state を構築し、timer preemption の下で複数の active user task record を走らせ、
parent-child metadata を保持し、running smoke task image を `execve` で置換し、終了済み user
address space と scheduler-owned kernel stack を reclaim できます。
kernel-internal な `kernel::process::spawn_user_program` helper は、filesystem executable path から
initial user task record までの boot-visible path を所有します。一方で filesystem lookup、ELF
mapping、address-space construction、scheduler metadata は既存 module の所有のままです。
`kernel::process::UserProgramEntryVectors` は、spawned program が使う borrowed `argv` / `envp`
slice を user stack construction 前に表す named representation です。
spawn helper は、task record 作成前に executable path lookup failure と image-buffer allocation
failure を stable な errno-facing result に分類します。user-visible `spawn` syscall と no-std
wrapper は、path-only compatibility と bounded `argv` / `envp` child launch surface を
smoke と shell bring-up 向けに公開します。
scheduler diagnostics は spawned origin path を current image path と別に保持するため、後続の
successful `execve` で `path=` が変わっても、`origin=` は task record を作った program を示し続けます。
`tasks` console command は user image ごとの最後に観測した `execve` replacement state も表示します。
successful candidate publication 後は `published`、prepared candidate が publish 前に破棄された場合は
`candidate_dropped` になります。

一方で、一般的な user-created process lifecycle はまだ未完成です。current working directory は
task metadata の所有になり、relative path は current task の directory から解決され、successful
`execve` は image replacement 後もその directory を保持します。`chdir` と `getcwd` の syscall
wrapper は、この task-owned directory を no-std userland code に公開します。scheduler-spawned child task は、
task creation 時点の parent current working directory をコピーします。現在の user-visible `spawn`
surface は、その directory で executable path を1つ解決し、bounded `argv` / `envp` vectors を
stage し、現在の descriptor inheritance selection を記録してから child を即座に active set へ入れます。
blocking `waitpid` は、matching child exit record が retained されるまで parent task を sleep させます。
descriptor table は process-owned task metadata になりました。user file descriptor syscall は current
task の table を操作し、spawned child は parent table から close-on-exec filter 済みの copy を受け取ります。
storage smoke は、parent が exit しても child がまだ alive のまま残る orphan boundary も cover します。
現在の runtime は、その child relationship を initial process である task `0` へ reparent してから
child exit を reap します。
descriptor close-on-exec metadata と successful-`execve` close behavior は、exec している process の
descriptor table に適用されます。`waitpid` の syscall number、option constant、no-std userland wrapper、
selector validation、no-child `ECHILD` path、parent task identifier で key 付けされた
scheduler-owned child exit record は実装済みなので、後続の child-exit 実装は安定した ABI target に
向けて進められます。最小 no-std `user_shell` binary は userland target set に入り、storage smoke disk
image に `/disk/bin/user_shell` として含まれ、storage smoke lifecycle gate の後に起動されます。
現在の shell は stdin を固定バッファへ1回読み、heap-free whitespace tokenization を検証し、fixed-buffer
`argv` を構築し、`/disk/bin/file_demo --shell-command-smoke` を `spawn` と `waitpid` で実行します。
standard input はまだ `/dev/null` なので EOF を検出して正常終了します。keyboard-backed stdin で interactive
lifetime を持たせる作業は未完了です。

## first stable process model

ManaOS は、最初の stable user process model として `spawn` plus `execve` を選びます。
最初の user-visible launch operation は、executable path、bounded `argv` / `envp` staging、
継承した process metadata、新しく構築した address space から child task を直接作ります。
child は後から `execve` で自分自身を置き換えられますが、process identifier、parent identifier、
waitability、current working directory は変わりません。

minimal `fork` は意図的に後回しにします。正しい `fork` には、公開前に address-space copy plan が
必要です。page-table frame ownership は clone 可能または共有可能である必要があり、writable user page は
eager copy または copy-on-write state が必要で、private mapping record には parent/child ownership の
明確な規則が必要です。さらに kernel stack と saved trap frame state は、1つの task にだけ属する
execution state を alias せずに複製しなければなりません。ManaOS にはまだこれらの
address-space lifecycle state がありません。

POSIX `fork` と比べると、最初の ManaOS model は1つの syscall から二度 return せず、caller の
address space 全体を複製せず、child に任意の in-memory user state を保持しません。代わりに、
明示的な argument と選択された executable entry point から child を開始します。一方で shell と
wait logic に必要な process property、つまり parent-child metadata、inherited current working
directory、close-on-exec aware descriptor inheritance、stable exit status、`waitpid` collection は保持します。

deferred `fork` work は spawn syscall surface からではなく、Phase 2 の address-space copy plan TODO から
始めます。その plan ができるまでは、shell と runtime launch helper は `spawn` plus `execve` を対象にします。

## `waitpid` syscall contract

`waitpid` は、parent process が scheduler-internal exit record を userland へ露出せずに、
終了した child process を観測して reap するための syscall です。ManaOS は Linux-compatible な
`wait4` syscall number を `SYS_WAITPID` として予約し、最初はより狭い `waitpid` argument subset
だけを扱います。

syscall ABI slice は ManaOS の通常の syscall register convention を使います。

- `rdi`: process identifier selector。正の値はその child process identifier に一致します。
  `WAIT_ANY` (`-1`) は任意の child に一致します。最初の subset では process-group selector は
  サポートしません。
- `rsi`: 32-bit wait status word への user pointer。null pointer は許可し、status storage を
  省略します。
- `rdx`: option bit。`0` は blocking wait、`WNOHANG` は matching child が exit していない場合に
  即時 return します。それ以外の option bit は `-EINVAL` を返す方針です。

現在の kernel dispatch は、`WAIT_ANY` と正の child process identifier を受け付け、未対応の
option bit と process-group selector は `-EINVAL` で拒否し、current user task に matching child が
存在しない場合は `-ECHILD` を返します。matching child に waitable exit record が既にある場合は、
その record を collect し、child process identifier を返し、status pointer が non-null なら normal
wait status word を格納します。`WNOHANG` は matching child が存在しても waitable な matching child
exit がまだない場合に `0` を返します。blocking wait は scheduler-owned wait request を parent task に
保存し、syscall frame 保存後にその task を block し、matching child exit record が retained されたら
parent を wake して saved syscall frame の `rax` に child process identifier を入れて resume します。
non-null status pointer は block 前に validate し、waiting parent の address space へ戻った後に書き込みます。
ManaOS にはまだ user interrupt policy がないため、この syscall は `-EINTR` を返しません。storage smoke は
no-std userland wrapper 経由で no-child と明示的な non-child selector path、explicit `argv` / `envp`
付き spawned child に対する pending `waitpid(WNOHANG) == 0`、その後の blocking `waitpid(WAIT_ANY)` reap と
nonzero status encoding を検証します。

scheduler-backed contract:

- 成功時は reaped child process identifier を返します。
- status pointer が non-null の場合、normal process exit status は `(exit_code & 0xff) << 8`
  として格納します。
- `WNOHANG` で matching child は存在するが reap 可能な exited child がない場合は `0` を返します。
- option `0` で matching child は存在するが reap 可能な exited child がない場合は parent を block します。
- child exit status は、成功した reap がちょうど一度だけ消費するまで保持します。
- address-space と kernel-stack resource は、scheduler が exit status を保持し、task を active user set から外し、
  kernel address space へ戻った後に reclaim します。
- wait collection は reclaimed runtime resource に依存しません。`waitpid` は scheduler metadata と child exit
  record だけを消費します。

## parent-child lifecycle states

現在の scheduler は、kernel task または user task を spawn するときに parent task identifier を
記録します。成功した `execve` は同じ task identifier と parent relationship を維持するため、parent
から見た child は image replacement によって別 process にはなりません。

現在の lifecycle state:

- Running or ready child: child task は parent identifier を持ち、live user runtime resource を
  所有しており、まだ waitable ではありません。
- Finished waitable child: `SYS_EXIT` が user task を `Finished` へ移し、parent-keyed child exit
  record に exit code を保持し、記録済み parent から観測可能にします。
- Collected child: parent-side collection path が保持済み exit code を一度だけ消費済みです。
  child exit record は collected として mark され、同じ child に対する二度目の collection は
  exit record を返しません。
- Reclaimed child resources: child exit record が保持され、task が再開不能になった後で、scheduler-owned
  cleanup path が finished child の user address space と kernel stack を解放済みです。この状態でも、
  wait collection は scheduler metadata と child exit record を使うため、child はまだ waitable な場合があります。

scheduler diagnostics は、exit status がまだ waitable な finished child を
`zombie_user_tasks` として、記録済み parent が collection 済みの child exit record を
`reaped_user_tasks` として公開します。既存の waitable/collected exit status counter は、
既存 smoke log との互換性のために残します。
`tasks` console command も per-task の `lifecycle` label を出力し、blocked task には
`waiting`、未 collection の child-exit record には `zombie`、collection 済みの
child-exit record には `reaped` を使います。

将来の general process model では、次の invariant を維持します。

- child は、現在記録されている parent に対してのみ waitable です。
- user parent が exit したら、scheduler はまだ waitable な child relationship を documented initial
  process へ reparent します。
- 成功した `execve` は process identifier、parent identifier、waitability を変えません。
- child exit status は、成功した parent reap がちょうど一度だけ消費するまで観測可能です。
- `waitpid(WNOHANG)` が `0` を返してよいのは、caller に matching child が存在し、reap 可能な
  exited matching child がまだない場合だけです。
- address-space と kernel-stack reclamation は、parent が reap する前に exit status を消してはいけません。
- orphan handling は明示し続けます。ManaOS が dedicated user `init` を起動するまでは、task `0` を
  reparented child の initial process とします。

現在の initial-process reparenting policy は scheduler-owned です。user task が exit するとき、scheduler は
recorded parent が exiting task である live user child と uncollected child exit record を task `0` へ移します。
reparenting は child task identifier、current working directory、address space、blocked/runnable state、
retained exit status を維持します。parent-exit smoke path は、parent より後に child が finish するケースを作り、
child が実行を続けることと、その child exit を task `0` 経由で reap できることを確認します。

## `execve` kernel contract

`execve` は、process identity と parent-child relationship を保ったまま、現在の process image を
置き換える syscall です。

syscall ABI slice は ManaOS の通常の syscall register convention を使います。

- `rdi`: NUL 終端 executable path への user pointer。
- `rsi`: NUL 終端 `argv` pointer array への user pointer。
- `rdx`: NUL 終端 `envp` pointer array への user pointer。

shared syscall number と no-std userland wrapper は実装済みです。kernel は executable path、
`argv`、`envp` を user pointer validation 経由で staging し、current filesystem namespace で executable
を解決し、ELF metadata を検証し、replacement candidate を構築し、prepared address space と trap frame を
scheduler 経由で publish し、old instruction pointer へ戻れなくなってから old user image を reclaim します。

kernel-side contract:

- process state を変更する前に executable path をコピーします。
- `argv` と `envp` array は user pointer validation helper を通してコピーします。
- `argv == NULL` は empty argument vector として扱います。
- `envp == NULL` は empty environment vector として扱います。
- path byte、argument count、environment count、total copied argument/environment byte は、
  allocation や stack construction の前に named constant で上限を設けます。
- path は current process filesystem namespace で解決します。relative path は task-owned current
  working directory から解決します。
- directory target は `-EISDIR` で拒否します。
- missing target は `-ENOENT` で拒否します。
- unsupported device target は `-EOPNOTSUPP` で拒否します。
- non-ELF または unsupported ELF image は、別の executable-format errno を入れるまで `-EINVAL` で
  拒否します。
- ELF validation と mapping policy は既存の user ELF loader を再利用します。
- コピー済みの `argv` / `envp` string と pointer array から新しい user stack を構築します。
- 成功時は current process identifier を保持します。
- 成功時は parent process identifier と waitable-child relationship を保持します。
- successful image replacement 後も、task-owned current working directory を保持します。
- open descriptor は default で継承し、replacement image が publish された後で close-on-exec と
  mark された descriptor だけを閉じます。
- old image に属する saved user trap frame、image-scoped syscall trace state、sleep/block state、
  pending user mapping record、heap break state、executable mapping metadata は reset します。
- executable image、heap start、user stack、initial trap frame の準備が完了するまで、新しい
  address space を publish してはいけません。

成功した `execve` は old user instruction pointer へ戻りません。次の user resume は new image entry
point と new stack state から始まります。失敗時は negative errno を old image に返し、old process
image は runnable のまま残します。

## argument and environment staging

`execve` は、partially installed process state が見えている間に user memory を歩いてはいけません。
安全な順序は次の通りです。

1. path、pointer array、string 本体を bounded kernel-owned staging storage にコピーする。
2. executable target と loadable ELF metadata を検証する。
3. staged data から new address space、user mapping、heap start、user stack を構築する。
4. scheduler-owned lifecycle transition で prepared image を一度に publish する。

最初の実装では、既存の initial-entry stack support に近い小さな固定上限を使います。argument count、
environment count、total copied string storage は小さく保ち、後から増やす場合は ABI と smoke test の
変更として扱います。

現在の staging は、既存の 256-byte path cap、8 `argv` entries、8 `envp` entries、NUL terminator を
含む 4096 total copied argument/environment string bytes を使います。invalid user pointer は
`-EFAULT`、count または byte limit overflow は `-E2BIG` を返します。

現在の path validation は absolute path をそのまま解決し、relative path は task-owned current
working directory から解決します。temporary descriptor で regular file contents を読み、missing
path は `-ENOENT`、directory は `-EISDIR`、device node は `-EOPNOTSUPP`、invalid ELF metadata は
`-EINVAL` で拒否します。valid image は candidate address space に map され、byte-preserving な
`argv` / `envp` stack content を構築したあと、current task の address space、heap state、private
mapping state、saved user trap frame を置き換えて publish します。

## address-space publication and rollback

new image が完全に構築されるまで、old image が authoritative です。partially built address space を
task record に install したり、schedule したり、`tasks` diagnostics に active として見せたりしては
いけません。

失敗時は candidate image 用に確保したすべての resource を解放します。

- candidate user PML4 と page-table frame。
- candidate ELF segment frame。
- candidate user heap metadata と mapped heap frame。
- candidate private mapping record と frame。
- candidate user stack frame と guard reservation。
- copied path、`argv`、`envp` 用の kernel staging buffer。
- image loading のためだけに open した descriptor reference。

失敗時は old address space、old trap frame、old user stack、old heap state、old private mapping、
current process ID、parent ID、inherited descriptor を変更しません。

現在の runtime path は successful publication を実際に通します。kernel は candidate address space を作り、
ELF segment を map し、candidate user stack と trap frame を準備し、`kernel::task` 経由で task record を
swap し、syscall stack 上の trap frame を new image entry state で上書きし、owner-checked frame allocator
path で old address space を reclaim します。candidate construction はまだ panic-on-OOM なので、一般的な
process facility として使う前に fallible にする必要があります。

成功時の swap は scheduler lifecycle transition が所有します。

1. task の address-space root、heap bookkeeping、private mapping bookkeeping、sleep state、initial resume
   trap frame を置き換える。
2. new image trap frame を syscall stack frame へ書き戻す。
3. internal successful `execve` sentinel を返し、syscall dispatch が old-image return value を書かないようにする。
4. old image へ戻る return path が残っていないことを確認してから、old user memory と mapping record を
   reclaimable にする。
5. finished-task cleanup と同じ owner-checked frame allocator path で old image resource を reclaim する。
6. old image reclaim と new image publication の diagnostics を記録する。

## descriptor inheritance

descriptor は close-on-exec flag が付いていない限り、成功した `execve` の後も継承します。
storage smoke は old image で executable file を first non-standard descriptor として open し、
new image が同じ descriptor number を close できることを検証します。

process-owned descriptor table は、open file ごとに close-on-exec metadata を記録します。user-visible な
`OPEN_CLOSE_ON_EXEC` flag は、successful `execve` cleanup 対象として descriptor を mark します。
unmarked descriptor は default で descriptor number と offset を保持し、marked descriptor は new image が
実行可能になってから閉じます。

kernel は boot-time diagnostics、console command、temporary executable image loading 用に別の descriptor
table を持ちます。user-facing descriptor syscall はその kernel table ではなく current task の process table を
操作します。

spawn descriptor inheritance は、broader general spawn の前に次の target policy を使います。

- child は、child executable を image loading 用に open する前に取得した parent descriptor table snapshot から
  descriptor を継承します。
- standard descriptor `0`, `1`, `2` は open されていれば継承します。spawned program で欠けた
  standard descriptor を初期化する作業は別の file-descriptor surface TODO です。
- non-standard descriptor は close-on-exec が mark されていない場合だけ default で継承します。
- inherited descriptor は descriptor number、file offset、file metadata、read/write capability を保持します。
- child executable を読むために kernel が一時的に開く descriptor は inherited set に入りません。
- 最初の spawn syscall surface は file-actions、descriptor duplication、selective close list を公開しません。
  それらは後続の shell redirection と descriptor-surface work に属します。

現在の runtime は、child image loader が temporary executable descriptor を開く前に task-owned spawn
inheritance snapshot を記録します。その後 scheduler は、parent snapshot から close-on-exec descriptor を
取り除いた process-owned descriptor table を child task に渡します。

## diagnostics and smoke coverage

現在の runtime diagnostics は、最初の successful replacement path を対象にしています。

- storage smoke は `/disk/bin/smoke_demo` からの successful self-replacement と、old image が再開しないことを
  検証します。
- storage smoke は user task の current working directory を `/disk` に変更し、relative self-`execve` と
  post-exec の relative `file_demo` replacement が preserved directory から解決されることを検証します。
- storage smoke は `chdir` 後に `getcwd` が task-owned `/disk` directory を返すことと、
  小さすぎる user buffer に `ERANGE` を返すことを検証します。
- storage smoke は kernel-internal な `spawn_user_program` helper 経由で user program を起動し、
  filesystem path loading、ELF mapping、initial argv/envp stack construction、scheduler task creation が
  1つの path を共有することを検証します。
- storage smoke は、scheduler-spawned user task が task creation 時点の parent current working directory を
  継承することを検証します。
- storage smoke は、helper が initial user stack を構築する前に staged entry vector count を assert します。
- storage smoke は successful spawn task creation の前に、missing、relative、directory、device、
  non-ELF target、image-buffer allocation failure の stable な spawn errno mapping を assert します。
- storage smoke は、distinct な `smoke_demo` parent task 3つと、spawn/wait coverage と
  parent-exit coverage 用の marker 付き `file_demo` parent 2つを spawn し、すべてをまとめて
  active set に入れる前提を assert します。
- storage smoke は user-visible `spawn_with_vectors` wrapper を使い、no-std userland から child を
  spawn し、child image 内で `argv` / `envp` を検証して、その child が実行中の間は
  `waitpid(WNOHANG) == 0` になり、後で child exit status を nonzero status としてちょうど一度だけ
  collect できることを検証します。
- storage smoke は、もう1つの userland parent が child を spawn して、その child が finish する前に
  parent が exit する case も実行します。child は通常の user lifecycle で exit し、task `0` へ reparent され、
  initial process 経由で reap されることを確認します。
- storage smoke は、finished child の address space と scheduler kernel stack が reclaim 済みでも、
  waitable child exit が `lifecycle=zombie` として観測可能なまま残ることを検証します。
- storage smoke は `sys_spawn` が child image loader の temporary executable descriptor を open する前に出す
  descriptor-inheritance snapshot を assert します。この snapshot は parent process table から出され、
  process-table path であることを明示します。
- storage smoke は、同じ task が `execve` で current image を置き換えた後も、`tasks` output が
  original spawn path を `origin=` として保持することを assert します。
- storage smoke は successful self-`execve` で継承された unmarked descriptor が new image でも使えることを
  検証します。
- storage smoke は `OPEN_CLOSE_ON_EXEC` 付きで open した descriptor が successful image replacement 中に
  閉じられたことを示す kernel log を assert します。
- storage smoke は post-exec smoke image を `/disk/bin/file_demo` へ置き換えることで、replacement が
  self-`execve` に限定されないことを検証します。
- storage smoke は experimental `user_shell` ELF が disk image に存在し、`/disk/bin/user_shell` として
  登録され、lifecycle gate 後に起動され、whitespace tokenization を検証し、
  `/disk/bin/file_demo --shell-command-smoke` を absolute path execution で起動して wait し、stdin EOF 後に
  initial process 経由で collect されることを検証します。
- serial log は `User image replaced by execve` と `execve image published` を old-image reclaim count 付きで
  記録します。
- scheduler smoke は、post-exec image が exit する前に `execve` が heap と private mapping bookkeeping を
  reset することを検証します。
- scheduler smoke は、successful replacement が per-task の `last_execve_state` diagnostic を
  `published` に更新し、replacement history のない spawned task は `none` のまま残ることを検証します。
- storage smoke は、current user task に child がない場合と、正の process identifier が child ではない場合に、
  `waitpid` が `-ECHILD` を返すことを検証します。
- storage smoke は、parent-keyed child exit record を保持する scheduler log line と、その record を一度だけ
  collect する log line を assert します。
- storage smoke は、selected-child wait collection、bootstrap child の zero-exit wait status encoding、
  userland-spawned child の nonzero wait status encoding を scheduler-owned child exit record 経由で
  assert します。
- storage smoke は、userland parent が `waitpid(WAIT_ANY)` で block し、spawned child の exit で wake し、
  parent address space に戻った後で nonzero wait status を書き、child task identifier を返して resume
  することを assert します。
- storage smoke は retained child count、collected child count、double-reap prevention を示す stable な
  wait lifecycle summary も assert します。
- `tasks` console command は、user task ごとの spawned origin path、current image generation、
  retained image path、last `execve` replacement state、last successful old-image reclaim count を表示します。

残りの runtime diagnostics では、より広い behavior を扱います。

- future post-candidate failure smoke は candidate frame をすべて返し、dropped replacement state を記録し、
  old image を runnable のまま保つことを証明します。

これらの diagnostics は、将来の CI smoke が interactive console output を parse せず検証できるよう、
stable serial log line を使います。
