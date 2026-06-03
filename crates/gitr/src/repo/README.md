# `repo`

High-level typed `Repository` API. Primary interface for agent workflows.

## Purpose

This module is the **public API surface** of `gitr`. It combines `GitCommand`
(spawning) and `parse` (parsing) into ergonomic, typed methods for common git
operations. Agents interact with git exclusively through `Repository`.

## Public API

### Types

- `Repository` — typed handle to a git repository.

### Methods

See `SPEC.md` for the full method list. Key categories:

- **Status:** `ensure_clean`, `status`, `changed_files`, `conflicted_files`
- **Branch:** `current_branch`, `branch_create`, `branch_delete`, `checkout`
- **Commit & Push:** `commit`, `push`, `fetch`
- **Worktree:** `worktree_add`, `worktree_remove`, `worktree_list`
- **Merge & Rebase:** `merge`, `merge_tree`, `rebase`, `rebase_continue`
- **Stash:** `stash`, `stash_pop`

## Invariants

- `Repository::open` validates that `.git` exists before returning.
- All methods are `async` and bound by the default 60-second timeout.
- Error message heuristics (e.g. "already exists" detection) are covered by tests.

## Consumers

- AI agents and automation tools — the primary target audience.
- Integration tests in `tests/repo.rs` — end-to-end validation.
