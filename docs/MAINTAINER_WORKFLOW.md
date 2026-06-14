# Direct Maintainer Branch Workflow

This workflow is only for maintainers and project-owned automation when no
external contributor is involved in the change. External contributors should
use the pull request workflow in [`CONTRIBUTING.md`](../CONTRIBUTING.md).

## When To Use It

Use the direct maintainer branch workflow for focused changes that can be fully
verified locally before entering `master`.

Do not use it for experimental work, broad refactors without a clear review
unit, or changes that still need external review before merge. Keep
`experimental/xxx` branches unmerged until the work is converted into a
verified `feature/xxx`, `fix/xxx`, `refactor/xxx`, or `docs/xxx` branch.

## Required Starting State

Start from a clean, current `master`:

```powershell
git switch master
git pull --ff-only origin master
git status --short --branch
git log -1 --oneline
```

The status output should show `master` tracking `origin/master` with no
uncommitted changes. If the project owner intentionally left changes to include,
confirm their scope before creating the task branch.

## Branch And Commit

Create one focused task branch:

```powershell
git switch -c docs/example-workflow
```

Use a branch prefix that matches the change:

- `feature/xxx` for a single feature unit.
- `fix/xxx` for a bug fix.
- `refactor/xxx` for restructuring without behavior changes unless stated.
- `docs/xxx` for documentation-only work.

Commit messages must be English and should be concise imperative summaries:

```powershell
git add <changed-files>
git commit -m "Document direct maintainer workflow"
```

## Local Verification

Run the narrowest relevant checks first, then broader checks when the change
crosses kernel, userland, architecture, or runtime boundaries.

For documentation-only changes:

```powershell
git diff --check
```

For Rust code changes:

```powershell
just fmt
cargo check
cargo check --target x86_64-unknown-uefi
cargo clippy --all-targets --all-features -- -D warnings
```

For changes that touch kernel/userland boundaries, architecture wiring, syscalls,
memory ownership, interrupt routing, storage, filesystem behavior, scheduler
behavior, or boot-visible runtime behavior:

```powershell
just lint
just storage-smoke
```

Record any skipped check and the reason before merging.

## Merge, Push, And Cleanup

Merge only after the task branch is verified:

```powershell
git switch master
git merge --ff-only docs/example-workflow
git push origin master
git branch -d docs/example-workflow
```

If the task branch was pushed to `origin`, delete it after `master` is pushed:

```powershell
git push origin --delete docs/example-workflow
```

If `--ff-only` fails, inspect the branch history instead of forcing the merge.
Do not use destructive commands to repair the history unless the project owner
explicitly requests that operation.

After cleanup, verify the final state:

```powershell
git status --short --branch
git log -1 --oneline
```

`master` should be clean, pushed, and pointing at the verified commit.
