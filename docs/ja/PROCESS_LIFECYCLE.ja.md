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
parent-child metadata を保持し、終了済み user address space と scheduler-owned kernel stack を
reclaim できます。

一方で、一般的な user-created process lifecycle はまだ未完成です。`execve`、
user-visible child creation、`waitpid`、process-owned current directory、close-on-exec descriptor
metadata は今後の作業です。

## `execve` kernel contract

`execve` は、process identity と parent-child relationship を保ったまま、現在の process image を
置き換える syscall です。

syscall ABI slice は ManaOS の通常の syscall register convention を使います。

- `rdi`: NUL 終端 executable path への user pointer。
- `rsi`: NUL 終端 `argv` pointer array への user pointer。
- `rdx`: NUL 終端 `envp` pointer array への user pointer。

shared syscall number と no-std userland wrapper は予約済みです。kernel は executable path、
`argv`、`envp` を user pointer validation 経由で staging し、current filesystem namespace で executable
を解決し、ELF metadata を検証し、未公開の replacement candidate を構築して rollback したうえで、
現時点では unsupported runtime result を返します。成功時の image publication path はまだ未実装です。

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
- open descriptor は default で継承し、close-on-exec metadata が入った後だけ、その flag を持つ
  descriptor を閉じます。
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
`-EOPNOTSUPP`、invalid ELF metadata は `-EINVAL` で拒否します。valid image は unpublished candidate
address space に map され、byte-preserving な `argv` / `envp` stack content を構築したあと、`execve`
がまだ `-ENOSYS` を返す間に rollback します。

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

現在の unsupported valid-image path はこの rule を実際に通します。kernel は candidate address space
を作り、ELF segment を map し、candidate user stack と trap frame を準備し、candidate address space
を destroy して、frame-owner total が build 前 snapshot と一致することを assert してから old image に
`-ENOSYS` を返します。

成功時の swap は scheduler lifecycle transition が所有します。

1. replacement commit 中は current user task の preemption を閉じる。
2. task の address-space root と initial resume trap frame を置き換える。
3. old image へ戻る return path が残っていないことを確認してから、old user memory と mapping record を
   reclaimable にする。
4. finished-task cleanup と同じ owner-checked frame allocator path で old image resource を reclaim する。
5. old image reclaim と new image publication の diagnostics を記録する。

## descriptor inheritance

descriptor は close-on-exec flag が付いていない限り、成功した `execve` の後も継承します。
close-on-exec flag はまだ存在しないため、最初の descriptor implementation step では既存の descriptor
number や offset を変えずに metadata を追加します。

close-on-exec が存在する場合も、descriptor を閉じるのは new image が publish 可能になってからです。
descriptor close が内部的に失敗した場合は、曖昧な partially replaced process を残すより、
context 付き panic として扱います。

## diagnostics and smoke coverage

最初の runtime implementation では、広い behavior より先に diagnostics を追加します。

- `tasks` output は last successful image path、current image generation、`execve` replacement が
  building / active / failed のどれかを表示します。
- storage smoke は `/disk` からの successful replacement を証明します。
- failure smoke は missing path と directory target の error を証明します。
- address-space smoke は failed replacement が candidate frame をすべて返し、old image を runnable のまま
  保つことを証明します。

これらの diagnostics は、将来の CI smoke が interactive console output を parse せず検証できるよう、
stable serial log line を使います。
