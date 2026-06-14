# Virtual Filesystem

この文書は [`../FILESYSTEM.md`](../FILESYSTEM.md) の日本語版です。ManaOS の virtual
filesystem が path をどのように正規化し、FAT32 backend とどう接続するかを説明します。

ManaOS は storage を layer に分けて扱います。AHCI が block device を公開し、GPT が partition を
選び、FAT32 が filesystem backend を提供し、virtual filesystem が kernel console と syscall に
安定した path / descriptor surface を提供します。

## ownership

- `kernel::driver::storage` は block-device registration と sector I/O を所有します。
- GPT / FAT32 parser は on-disk structure validation を所有します。
- `kernel::filesystem` は mount point、canonical path、file descriptor、directory handle、
  errno-facing filesystem error を所有します。
- console command と syscall は filesystem API を使います。FAT32 や partition structure を
  直接 parse してはいけません。

## path normalization

kernel virtual filesystem は、path を canonical absolute form で保持します。

正規化ルール:

- 連続する slash は1つに畳みます。
- `.` component は無視します。
- `..` は直前の component を削除しますが、`/` より上へは出ません。
- trailing slash は別 path を作りません。
- 正規化結果が空の場合は `/` として扱います。

例:

- `/dev//console` は `/dev/console` になります。
- `/disk/../README` は `/README` になります。
- `/dev/` は `/dev` になります。

console は relative path を current working directory に対して解決してから、virtual
filesystem へ渡します。これにより、VFS 側は absolute canonical path を前提にできます。

normalization は security boundary の一部です。`..` は `/` より上へ出てはいけません。
normalized path は console command、syscall、future user shell execution の間で同じ意味を
持つ必要があります。

## mount model

VFS は explicit mount table を保持します。各 mount point は以下を持ちます。

- canonical absolute mount path。
- backend implementation。
- read-only / writable flags。
- directory traversal behavior。
- common filesystem surface に公開する metadata と descriptor operation。

mount flag は mutation operation の前に検査します。low-level AHCI write が存在していても、
FAT32 backend は現時点では read-only です。

## FAT32 backend

`/disk` に mount される FAT32 file は read-only backend file です。virtual filesystem は
metadata と read callback を保持します。file descriptor を read したときに storage subsystem
経由で file byte を取得し、boot 時に file 全体を heap buffer へコピーする設計ではありません。

FAT32 backend は以下を担当します。

- boot sector と FSInfo data の validation。
- long file name を含む directory entry read。
- directory / file read での cluster chain traversal。
- invalid cluster と cluster-chain loop の rejection。
- VFS format での file metadata exposure。
- backend failure から filesystem error への mapping。

## file descriptor と directory

file descriptor は current offset と backend-specific open state を所有します。regular file read は
offset を進めます。`lseek` は validated seek mode に従って offset を更新します。directory handle は
`getdents64` 形式の iteration を行い、entry を重複させずに listing を再開できる offset state を
保持します。

descriptor layer は syscall errno mapping を集約します。backend error が ad hoc な console string や
storage-driver boolean として漏れないようにします。

## 設計上の意味

- path normalization は console command、syscall、future user shell の間で一貫した挙動を
  作るための境界です。
- FAT32 backend は storage driver、partition parser、VFS、file descriptor layer の接続点です。
- read-only mount と writable mount の flag は、将来 FAT32 mutation を入れる前に error policy を
  明確にするために必要です。
- `..` が mount root を越える場合の扱いは security boundary でもあるため、曖昧にしません。

## mutation policy

write-capable FAT32 support は separate verified step として追加します。disk image を変更する前に、
以下を文書化し、実装します。

- directory entry と FAT update の transaction boundary。
- partial allocation failure の rollback behavior。
- modified FAT sector の flush または write-through semantics。
- journaling がない間の corruption assumption。
- QEMU disk image 上で file を create/read/delete する smoke coverage。

この policy が実装されるまでは、read-only mount への write attempt は一貫して失敗する必要があります。
