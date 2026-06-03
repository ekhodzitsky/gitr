# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `command.rs` now uses real `tokio::time::timeout`, retry logic with exponential
  backoff for network errors, and non-interactive git environment variables.
- `parse.rs` updated to match omk logic: proper `merge-tree` conflict detection,
  `parse_log`, `parse_remotes`, `parse_has_diff`.
- `repo.rs` adds `diff_files`, `push_force`, `open_worktree`.
- `tracing` feature is now optional; `repo.rs` compiles without it.

### Fixed

- README worktree example now uses real `Repository::open_worktree`.
- `Repository::open` is now truly async (uses `tokio::fs::canonicalize`).

### Testing

- Ported 30+ unit tests from omk: status, branch, worktree, merge-tree, commit,
  stash, diff, fetch, checkout, push, merge, rebase, add error paths.
- Added hermetic `command` tests: timeout, retry, is_retryable, output fields.
- Added parse-only unit tests with sample porcelain output.
- Coverage: 89.1% (335/376 lines).

### Added

- `ScriptedRunner` — hermetic test runner that replays scripted responses.
- `Repository::diff_shortstat` and `parse_diff_shortstat` for structured diff stats.
- Full rustdoc coverage; `#![warn(missing_docs)]` enforced.

### CI

- Added `typos` job.
- Added `dependency-review` job for PR gating.
- Added `cargo-hack` job for feature powerset testing.
- Coverage threshold raised to 85% (actual: 89.7%).

## [0.1.0] - 2026-06-02

### Added

- Initial release with async typed git CLI wrapper.
- `Repository::open`, `current_branch`, `head_commit`, `ensure_clean`.
- Worktree operations: `worktree_add`, `worktree_remove`, `worktree_list`.
- Branch operations: `branch_create`, `branch_delete`, `branch_exists`, `checkout`.
- Commit, push, fetch, stash, merge, rebase operations.
- Read-only merge-tree conflict detection (`merge_tree`).
- Structured error enum (`GitError`) with typed variants.
- Porcelain parsers for `status`, `worktree list`, `branch`, `merge-tree`.
