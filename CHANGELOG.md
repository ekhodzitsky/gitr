# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Streaming log API** (`stream` feature): `Repository::log_stream()` returns an async `Stream` of `GitLogEntry` without buffering the entire log in memory.
- **Submodule API**: `submodule_list`, `submodule_add`, `submodule_update`, `submodule_deinit`, `submodule_sync`.
- **GPG support**: `commit_signed` and `verify_commit` for signed commits.
- **Clone/init API**: `Repository::init` and `Repository::clone` static constructors.
- **New query methods**: `ls_files`, `diff_cached`, `archive`, `grep`, `describe`, `clean`.
- `GitSubmodule`, `GitVerification`, and `GitGrepResult` types.
- `tokio-stream` optional dependency (enabled via `stream` feature).
- `Repository::with_cancel` to attach a cancellation token.
- `gitr-cli` now exposes `init`, `clone`, `commit`, `grep`, `ls-files`, `submodule`, `config`, `tag`, `stash`, `show`, `archive`, `describe`, `clean`, `diff --cached`.
- `gitr-mcp` now exposes 25+ JSON-RPC methods including streaming log, submodule, GPG, grep, archive, and reset operations.
- **Structured diff** (`FileDiff`, `DiffHunk`, `DiffLine`, `DiffLineKind`): `diff_structured`, `diff_cached_structured`, and `diff_files_structured` parse unified diff output into typed hunks and lines.
- `parse_diff` porcelain parser with unit tests for new files, deletions, renames, and binary diffs.
- `gitr diff --structured --cached --json` CLI flag.
- `git_diff_structured` MCP method.
- **Structured blame** (`BlameLine`): `blame_structured` parses `git blame --line-porcelain` into per-line authorship entries.
- `parse_blame` porcelain parser with unit tests.
- `gitr blame --structured --json` CLI command.
- `git_blame_structured` MCP method.
- **Patch operations**: `format_patch` (parses `git format-patch --stdout` into `Vec<Patch>`), `apply_patch`, and `apply_patch_file` with dry-run support.
- `Patch` and `ApplyReport` types.
- `parse_format_patch` mbox parser with unit tests.
- `gitr format-patch <range>` and `gitr apply <patch-file> --check` CLI commands.
- `git_format_patch` and `git_apply_patch` MCP methods.
- **Reflog**: `reflog_list` and `reflog_expire` with `ReflogEntry` type.
- `parse_reflog` parser with unit tests.
- `gitr reflog list [ref] --json` and `gitr reflog expire <ref> --time` CLI commands.
- `git_reflog_list` and `git_reflog_expire` MCP methods.
- **Git hooks**: `hooks_list`, `hook_install`, `hook_remove`, `run_hook` with `Hook` and `HookOutput` types.
- `gitr hooks list --json`, `gitr hooks install <name> <script>`, `gitr hooks remove <name>`, `gitr hooks run <name>` CLI commands.
- `git_hooks_list`, `git_hook_install`, `git_hook_remove`, `git_run_hook` MCP methods.
- **Bisect**: `bisect_start`, `bisect_bad`, `bisect_good`, `bisect_reset`, `bisect_run` with `BisectState` type.
- `gitr bisect start --bad <sha> --good <sha>`, `gitr bisect bad [sha]`, `gitr bisect good [sha]`, `gitr bisect reset`, `gitr bisect run <command>` CLI commands.
- `git_bisect_start`, `git_bisect_bad`, `git_bisect_good`, `git_bisect_reset`, `git_bisect_run` MCP methods.
- **Git notes**: `notes_list`, `notes_show`, `notes_add`, `notes_remove` with `GitNote` type.
- `gitr notes list --namespace <ns> --json`, `gitr notes show <object> --namespace <ns>`, `gitr notes add <object> --message <msg> --namespace <ns> --force`, `gitr notes remove <object> --namespace <ns>` CLI commands.
- `git_notes_list`, `git_notes_show`, `git_notes_add`, `git_notes_remove` MCP methods.
- **Ignore/attributes evaluation**: `check_ignore` and `check_attr` with `GitAttr` type.
- `gitr check-ignore <path>...` and `gitr check-attr <path>... --attr <attr>...` CLI commands.
- `git_check_ignore` and `git_check_attr` MCP methods.
- **Bundle operations**: `bundle_create`, `bundle_list_heads`, `bundle_verify`, `bundle_unbundle`.
- `gitr bundle create <output> [refs...]`, `gitr bundle list-heads <path>`, `gitr bundle verify <path>`, `gitr bundle unbundle <path>` CLI commands.
- `git_bundle_create`, `git_bundle_list_heads`, `git_bundle_verify`, `git_bundle_unbundle` MCP methods.
- **Builder pattern**: `PushOptions`, `CommitOptions`, `RebaseOptions`, `MergeOptions`, `FetchOptions`, `CherryPickOptions` with `_opts` variants and default delegations.
- Old methods (`commit`, `push`, `fetch`, `merge`, `rebase`, `cherry_pick`) retain backwards-compatible signatures and delegate to `_opts` internally.
- **Streaming APIs** (`stream` feature): `grep_stream`, `blame_stream`, `ls_files_stream` return `impl Stream` for line-by-line processing without buffering entire output.
- **Git version detection**: `GitVersion` parsed from `git --version`, exposed via `Repository::git_version()`.
- Adaptive parsing warning when git version > 2.45 (via `tracing`).
- **Snapshot testing** (`insta`): 41 snapshot tests covering all porcelain parsers with recorded real-git output.
- **Property-based testing** (`proptest`): roundtrip and never-panics properties for `parse_diff_shortstat`, `parse_log_line`, `parse_grep`, `parse_status`, `parse_branches`, `parse_worktrees`, `parse_merge_tree`.
- **Bulk operations**: `hash_object`, `write_blob`, `mktree`, `write_tree` for object DB writes.
- **Object DB read**: `read_object`, `read_tree`, `read_blob`, `read_commit` with `ObjectContent`, `GitCommit`, `TreeEntry` types.
- **Index manipulation**: `read_index`, `update_index`, `remove_index` with `IndexEntry` type.
- **Git LFS**: `lfs_track`, `lfs_untrack`, `lfs_ls_files`, `lfs_lock`, `lfs_unlock` with `GitLfsFile` type.
- **Sparse checkout**: `sparse_checkout_init`, `sparse_checkout_set`, `sparse_checkout_add`, `sparse_checkout_disable`, `sparse_checkout_list`.
- **Hook sandboxing**: `run_hook_with_timeout` (custom timeout per hook), `run_hook_streaming` (real-time stdout/stderr via channels).
- **Metrics** (`metrics` feature): `gitr_commands_total` counter and `gitr_command_duration_seconds` histogram per command.
- `CatFileBatch` persistent process infrastructure for future `cat-file --batch` optimization.

### Changed

- `Repository::commit` and `GitApi::commit` now accept `no_verify: bool` to skip pre-commit hooks.
- `Repository::commit_signed` also accepts `no_verify: bool`.
- `GitApi` trait extended with all new methods for consistency.

### Fixed

- Removed unused `serde` dependency.
- `GitApi::commit` now accepts `paths` to match `Repository::commit`.
- `diff_files` no longer rejects non-UTF-8 paths (uses `to_string_lossy`).
- `init` and `clone` now capture stdout/stderr on failure for actionable diagnostics.
- Removed dead code: `parse_has_diff` and redundant `#[allow(dead_code)]` annotations.
- Fixed missing re-exports in `lib.rs`: `GitSubmodule`, `GitVerification`, `GitGrepResult`.
- Fixed `parse_status` non-null-terminated branch: files simultaneously staged and unstaged (e.g. `MM`) are now correctly reported in both lists.
- Fixed `is_nothing_to_commit` to ignore untracked files (matches git semantics).
- `changed_files`, `untracked_files`, `has_untracked_files`, and `ensure_clean` now reuse `self.status()` so they benefit from the in-memory cache when enabled.
- `log_stream`, `init`, and `clone` now set the same non-interactive environment variables as `GitCommand::run`.
- `log_stream` redirects stderr to `/dev/null` to avoid pipe-buffer deadlocks.
- `parse_remotes` now deduplicates fetch/push URLs for the same remote instead of returning duplicate entries.

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
