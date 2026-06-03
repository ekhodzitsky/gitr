# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-06-03

### Added

- **`GitApi` trait** — high-level async trait for all repository operations.
  `Repository` implements `GitApi`; downstream can use `Box<dyn GitApi>` for
  mockability.
- **Agent helpers** — `is_merge_conflict`, `is_nothing_to_commit`,
  `has_untracked_files`, `GitError::is_retryable()`.
- **`ScriptedRunner`** — hermetic test runner that replays scripted responses.
- **`parse_diff_shortstat`** and `Repository::diff_shortstat` for structured diff stats.
- **`parse_status_z`** and `Repository::status_z` for null-delimited porcelain.
- Full rustdoc coverage; `#![warn(missing_docs)]` enforced.
- `CONTRIBUTING.md` and `SECURITY.md`.

### Changed

- `command.rs` now uses real `tokio::time::timeout`, retry logic with exponential
  backoff for network errors, and non-interactive git environment variables.
- `parse.rs` updated to match omk logic: proper `merge-tree` conflict detection,
  `parse_log`, `parse_remotes`, `parse_has_diff`.
- `repo.rs` adds `diff_files`, `push_force`, `open_worktree`, `status_z`.
- `tracing` feature is now optional; `repo.rs` compiles without it.

### Fixed

- README worktree example now uses real `Repository::open_worktree`.
- `Repository::open` is now truly async (uses `tokio::fs::canonicalize`).

### Testing

- 66 tests (62 unit + 2 integration + 2 doc-tests).
- Hermetic `command` tests: timeout, retry, is_retryable, output fields.
- Parse-only unit tests with sample porcelain output.
- Coverage: 89.7% (381/425 lines).

### CI

- Added `typos`, `dependency-review`, `cargo-hack` jobs.
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
