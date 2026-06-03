# gitr — Current Tasks & Known Gaps

## In Progress

_None — this file is updated as work is claimed._

## Next Up

### Testing Infrastructure

- [x] **Hermetic unit tests** — Partial: `command` tests use shell scripts (no git binary).
  Full `ScriptedRunner`/`RecordingRunner` deferred to post-0.1.
- [x] **Parse-only unit tests** — Every parser has tests with sample output.
- [x] **Integration test un-ignore** — `tests/repo.rs` now runs without `#[ignore]`;
  skips gracefully when git is not in PATH.

### API Evolution

- [ ] **`GitApi` trait** — Extract `GitApi` trait from `Repository` for mockability
  in downstream consumers. `Repository` becomes the default impl.
- [ ] **Typed porcelain parsers expansion**
  - `diff --shortstat` → structured diff stats
  - `log --format` with `\x1f` delimiters → `Vec<GitLogEntry>`
  - `status -z` → null-delimited porcelain for paths with spaces
- [ ] **Agent helpers** — Convenience methods for common agent workflows:
  - `is_merge_conflict() -> bool`
  - `is_nothing_to_commit() -> bool`
  - `is_transient_fetch_error() -> bool`
  - `has_untracked_files() -> bool`

### Missing Features (vs. omk needs)

- [ ] **`merge_tree` enhancement** — Parse `tree_oid` from merge-tree output correctly.
- [ ] **Conflict classification** — Distinguish content vs. rename vs. delete conflicts.
- [ ] **Transient fetch detection** — Parse stderr for network-related fetch failures.
- [ ] **Worktree validation** — `open_worktree(path)` that validates the path is a
  registered worktree before returning a `Repository` handle.

### Documentation

- [x] **Module-level README.md** — Every `src/X/` module needs README with:
  - Purpose
  - Public API (types, traits, functions)
  - Consumers
  - Invariants
- [x] **SPEC.md** — Formal API contract and design decisions.
- [ ] **ROADMAP.md** — Staging for 0.2.0, 0.3.0, 1.0.0.

### Code Quality

- [ ] **Remove `#[allow(dead_code)]`** — `GitLogEntry`, `GitRemote`, `stderr`/`exit_code`
  in `CommandOutput` should be used or removed.
- [ ] **`run_with_timeout` actually uses timeout** — Currently `_dur` is unused;
  `tokio::time::timeout` must be applied.
- [x] **Clippy lint policy in `src/lib.rs`** — Add `#![warn(...)]` for Tier 1 lints.

## Done

- [x] Initial crate scaffolding
- [x] CI pipeline (matrix, deny, coverage, semver)
- [x] Core types (`GitStatus`, `GitMergeResult`, `GitWorktree`)
- [x] Core parsers (`status`, `worktrees`, `branches`, `merge-tree`)
- [x] `Repository` API surface (open, branch, worktree, commit, push, fetch, rebase, merge, stash)
- [x] `CHANGELOG.md` with per-PR discipline
