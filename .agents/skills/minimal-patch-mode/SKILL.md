---
name: minimal-patch-mode
description: Keep Rust OS/kernel code changes small, scoped, and reviewable in this ManaOS repo. Use for any requested code change; skip for pure explanation or documentation-only questions unless edits are requested.
---

# Minimal Patch Mode

## When To Use

Use this skill before any code change in this Rust no_std OS repository. Do not use it for pure explanation or read-only documentation questions unless the user asks for edits.

## Inputs To Inspect

- `AGENTS.md`, `CONTRIBUTING.md`, and the files directly touched by the request.
- `docs/ARCHITECTURE.md`, `docs/TASK_PRIORITY.md`, and relevant subsystem docs when changing boundaries.
- Current branch and worktree with `git status --short --branch`.
- Existing patterns near the target code before introducing new helpers or modules.

## Workflow

1. State the smallest behavior change that satisfies the request.
2. List the exact files likely to change; keep edits under `.agents/skills/` when creating skills.
3. Prefer existing module boundaries, naming, ownership comments, and APIs.
4. Avoid unrelated refactors, formatting churn, dependency updates, and broad renames.
5. Change one responsibility at a time; split larger work into follow-up slices.
6. Keep unsafe/kernel/concurrency-sensitive edits minimal. Successful compilation is not proof of correctness.
7. Run the narrowest relevant checks first, then broader checks if the change crosses boundaries.

## Repo-Specific Commands

- Format: `just fmt`
- Kernel check: `cargo check --target x86_64-unknown-uefi`
- General check: `cargo check`
- Lint and boundary checks: `just lint`
- Strict clippy: `cargo clippy --all-targets --all-features -- -D warnings`
- Boot smoke: `just storage-smoke`

## Safety Checks

- Do not modify unrelated files or generated artifacts.
- Do not use destructive git commands.
- Preserve `arch/` to `kernel/` dependency rules and `main.rs` composition-root wiring.
- Preserve English-only code, comments, and commit messages.
- Confirm public Rust items still have `///` docs and unsafe blocks still have nearby `// SAFETY:` comments.

## Done Criteria

- The diff is limited to the requested behavior.
- No unrelated refactor or dependency change is present.
- Relevant checks pass or skipped checks are explicitly justified.
- The final worktree state is understood and reported.

## Report Back

Report changed files, the smallest behavior change made, commands run, command results, and any follow-up recommendation that should not be silently implemented.
