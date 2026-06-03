# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Removed unused `serde` dependency.
- `GitApi::commit` now accepts `paths` to match `Repository::commit`.
- `diff_files` no longer rejects non-UTF-8 paths (uses `to_string_lossy`).

### Documentation

- Clarified that `open_worktree` is an alias for `open`.

## [0.3.0] - 2026-06-03

### Fixed

- `Repository::open` now returns `GitError::Io` when path canonicalization fails instead of silently falling back to the non-canonicalized path.
- `parse_merge_tree` now handles additional git conflict types: `rename/delete`, `modify/delete`, `delete/modify`, `rename/rename`, and `directory/file`.
- Removed redundant `"fatal: unable to access"` check in `is_retryable` (already covered by `"unable to access"`).

### Added

- `Repository::log()` and `Repository::remotes()` methods.
- `GitApi::log()` and `GitApi::remotes()` trait methods.
- `GitLogEntry` and `GitRemote` are now re-exported from the crate root.
- `test-utils` feature gate for `ScriptedRunner`.
- `CHANGELOG.md`.

### Changed

- `ScriptedRunner` is no longer available in the public API unless the `test-utils` feature is enabled.
- `#[allow(dead_code)]` on `CommandOutput` fields moved to struct-level with explanatory comment.

### Documentation

- Documented `diff_files` non-UTF-8 path limitation in rustdoc.

## [0.2.0] - 2026-05-27

### Added

- `GitApi` async trait for downstream mockability.
- `ScriptedRunner` hermetic test runner.
- Agent helpers: `is_merge_conflict()`, `is_nothing_to_commit()`, `has_untracked_files()`.
- `parse_status_z` for null-delimited porcelain (paths with spaces).
- `parse_diff_shortstat` with structured `DiffShortstat`.
- Full rustdoc coverage for all public types and methods.
- `CONTRIBUTING.md` and `SECURITY.md`.

### Changed

- Ported retry logic, `tokio::time::timeout`, non-interactive env, and backoff from `omk` git layer.
- `tracing` debug calls wrapped with `#[cfg(feature = "tracing")]` for `cargo-hack` compatibility.

## [0.1.0] - 2026-05-20

### Added

- Initial release: async typed git CLI wrapper.
- `Repository`, `GitCommand`, porcelain parsers, typed errors.
