# Virtual Filesystem

この文書は [`../FILESYSTEM.md`](../FILESYSTEM.md) の日本語版です。ManaOS の virtual
filesystem が path をどのように正規化し、FAT32 backend とどう接続するかを説明します。

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

## FAT32 backend

`/disk` に mount される FAT32 file は read-only backend file です。virtual filesystem は
metadata と read callback を保持します。file descriptor を read したときに storage subsystem
経由で file byte を取得し、boot 時に file 全体を heap buffer へコピーする設計ではありません。

## 設計上の意味

- path normalization は console command、syscall、future user shell の間で一貫した挙動を
  作るための境界です。
- FAT32 backend は storage driver、partition parser、VFS、file descriptor layer の接続点です。
- read-only mount と writable mount の flag は、将来 FAT32 mutation を入れる前に error policy を
  明確にするために必要です。
- `..` が mount root を越える場合の扱いは security boundary でもあるため、曖昧にしません。
