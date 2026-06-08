# `error`

Structured error enum for all git operations.

## Purpose

Every failure mode in `gitr` is represented as a typed `GitError` variant.
Callers can `match` on specific errors (e.g. `BranchNotFound`) to implement
fallback logic. This module is the **error boundary** of the crate.

## Public API

### Types

- `GitError` — enum covering all failure modes:
  - `NotARepo(PathBuf)` — path lacks `.git` directory.
  - `GitNotFound` — `git` binary not in `PATH`.
  - `CommandFailed { command, exit_code, stderr, stdout }` — non-zero exit.
  - `Timeout(Duration, String)` — command exceeded time budget.
  - `Dirty(String)` — working tree has staged/unstaged changes.
  - `BranchExists(String)` / `BranchNotFound(String)` — branch lifecycle.
  - `WorktreeExists(String)` — worktree already registered.
  - `MergeConflicts(Vec<String>)` — unresolvable conflicts detected.
  - `Io(String)` / `Parse(String)` — I/O or parse failures.

## Invariants

- Error messages are lowercase without trailing punctuation.
- The `From<std::io::Error>` impl preserves the cause chain.
- `CommandFailed` always includes the full command string for debugging.

## Consumers

- All modules in `src/` — every public function returns `Result<T, GitError>`.
- Downstream crates — match on variants for retry/fallback logic.
