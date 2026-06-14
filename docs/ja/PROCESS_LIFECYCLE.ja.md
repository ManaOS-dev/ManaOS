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
  descriptor table、filesystem error value を所有します。
- `kernel::elf` は user image の ELF validation と segment mapping policy を所有します。
- `kernel::memory` は user address-space の construction、publication、rollback、
  frame reclamation を所有します。
- `kernel::task` は process identifier、parent-child metadata、scheduler state、
  trap frame、exit record、lifecycle diagnostics を所有します。
- `main.rs` は boot-time smoke wiring の composition root です。process replacement policy の
  owner にはしません。

## current status

現在の kernel は、filesystem から user ELF image を読み込み、初期 `argc` / `argv` / `envp`
stack state を構築し、timer preemption の下で複数の active user task record を走らせ、
parent-child metadata を保持し、running smoke task image を `execve` で置換し、終了済み user
address space と scheduler-owned kernel stack を reclaim できます。

一方で、一般的な user-created process lifecycle はまだ未完成です。current directory の ownership、
user-visible child creation、scheduler-backed な `waitpid` の wait/reap state machine は今後の作業です。
descriptor close-on-exec metadata と successful-`execve` close behavior は、現在の global descriptor
table 向けに実装済みです。`waitpid` の syscall number、option constant、no-std userland wrapper、
selector validation、no-child `ECHILD` path、parent task identifier で key 付けされた
scheduler-owned child exit record は実装済みなので、後続の child-exit 実装は安定した ABI target に
向けて進められます。

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
存在しない場合は `-ECHILD` を返します。matching child が存在する場合は、blocking、nonblocking、
reap behavior が scheduler-owned exit record へ接続されるまで `-ENOSYS` を返します。ManaOS には
まだ user interrupt policy がないため、この syscall は `-EINTR` を返しません。storage smoke は
no-std userland wrapper 経由で no-child と明示的な non-child selector path を検証するため、後続の
behavior change は明示的に変わります。

残りの scheduler-backed contract:

- 成功時は reaped child process identifier を返します。
- status pointer が non-null の場合、normal process exit status は `(exit_code & 0xff) << 8`
  として格納します。
- `WNOHANG` で matching child は存在するが reap 可能な exited child がない場合は `0` を返します。
- child exit status は、成功した reap がちょうど一度だけ消費するまで保持します。
- address-space と kernel-stack resource は、scheduler-owned lifecycle policy 上で exit record が
  安全になってから reclaim します。

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
- Reclaimed child resources: current smoke lifecycle が child を再開しなくなった後で、scheduler-owned
  cleanup path が finished child の user address space と kernel stack を解放済みです。

scheduler diagnostics は、exit status がまだ waitable な finished child を
`zombie_user_tasks` として、記録済み parent が collection 済みの child exit record を
`reaped_user_tasks` として公開します。既存の waitable/collected exit status counter は、
既存 smoke log との互換性のために残します。

将来の general process model では、次の invariant を維持します。

- child は、parent exit 後の reparenting policy が文書化されるまでは、記録済み parent に対してのみ
  waitable です。
- 成功した `execve` は process identifier、parent identifier、waitability を変えません。
- child exit status は、成功した parent reap がちょうど一度だけ消費するまで観測可能です。
- `waitpid(WNOHANG)` が `0` を返してよいのは、caller に matching child が存在し、reap 可能な
  exited matching child がまだない場合だけです。
- address-space と kernel-stack reclamation は、parent が reap する前に exit status を消してはいけません。
- orphan handling は明示する必要があります。documented initial process へ reparent するか、orphan を
  生成しうる process model を拒否します。

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
- path は current process filesystem namespace で解決します。process-owned current directory が
  入るまでは、user `execve` は absolute path のみ受け付ける方針にします。
- directory target は `-EISDIR` で拒否します。
- missing target は `-ENOENT` で拒否します。
- unsupported device target は `-EOPNOTSUPP` で拒否します。
- non-ELF または unsupported ELF image は、別の executable-format errno を入れるまで `-EINVAL` で
  拒否します。
- ELF validation と mapping policy は既存の user ELF loader を再利用します。
- コピー済みの `argv` / `envp` string と pointer array から新しい user stack を構築します。
- 成功時は current process identifier を保持します。
- 成功時は parent process identifier と waitable-child relationship を保持します。
- current directory が process metadata に所有された後は、成功時に current working directory を
  保持します。
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

現在の path validation は absolute executable path だけを受け付け、temporary descriptor で regular
file contents を読みます。missing path は `-ENOENT`、directory は `-EISDIR`、device node は
`-EOPNOTSUPP`、invalid ELF metadata は `-EINVAL` で拒否します。valid image は candidate
address space に map され、byte-preserving な `argv` / `envp` stack content を構築したあと、current
task の address space、heap state、private mapping state、saved user trap frame を置き換えて publish
します。

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

現在の descriptor table は、open file ごとに close-on-exec metadata を記録します。user-visible な
`OPEN_CLOSE_ON_EXEC` flag は、successful `execve` cleanup 対象として descriptor を mark します。
unmarked descriptor は default で descriptor number と offset を保持し、marked descriptor は new image が
実行可能になってから閉じます。

現在の table はまだ per-process ではなく global なので、これは smoke lifecycle に必要な最小 metadata です。
将来の per-process descriptor table でも同じ rule を維持しつつ、exec している process の descriptor にだけ
適用する必要があります。

## diagnostics and smoke coverage

現在の runtime diagnostics は、最初の successful replacement path を対象にしています。

- storage smoke は `/disk/bin/smoke_demo` からの successful self-replacement と、old image が再開しないことを
  検証します。
- storage smoke は successful self-`execve` で継承された unmarked descriptor が new image でも使えることを
  検証します。
- storage smoke は `OPEN_CLOSE_ON_EXEC` 付きで open した descriptor が successful image replacement 中に
  閉じられたことを示す kernel log を assert します。
- storage smoke は post-exec smoke image を `/disk/bin/file_demo` へ置き換えることで、replacement が
  self-`execve` に限定されないことを検証します。
- serial log は `User image replaced by execve` と `execve image published` を old-image reclaim count 付きで
  記録します。
- scheduler smoke は、post-exec image が exit する前に `execve` が heap と private mapping bookkeeping を
  reset することを検証します。
- storage smoke は、current user task に child がない場合と、正の process identifier が child ではない場合に、
  `waitpid` が `-ECHILD` を返すことを検証します。
- storage smoke は、parent-keyed child exit record を保持する scheduler log line と、その record を一度だけ
  collect する log line を assert します。
- storage smoke は retained child count、collected child count、double-reap prevention を示す stable な
  wait lifecycle summary も assert します。
- `tasks` console command は、user task ごとの current image generation、retained image path、last successful
  old-image reclaim count を表示します。

残りの runtime diagnostics では、より広い behavior を扱います。

- `tasks` output は、candidate construction に fallible post-build failure point が入った後で、
  replacement building / failed state を表示します。
- future post-candidate failure smoke は candidate frame をすべて返し、old image を runnable のまま保つことを
  証明します。

これらの diagnostics は、将来の CI smoke が interactive console output を parse せず検証できるよう、
stable serial log line を使います。
