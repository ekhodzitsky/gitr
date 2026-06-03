# `types`

Shared data types for git operations.

## Purpose

This module contains the **value types** used across `gitr`. It has no logic —
only structs with public fields. Keeping types in a dedicated module prevents
circular dependencies between `repo`, `parse`, and `error`.

## Public API

### Types

- `GitStatus` — staged, unstaged, and untracked file lists.
- `GitMergeResult` — conflict detection result with optional tree OID.
- `GitWorktree` — path and branch for a registered worktree.
- `GitLogEntry` — SHA, message, author, timestamp (reserved for future use).
- `GitRemote` — name and URL (reserved for future use).

## Invariants

- All types implement `Debug`, `Clone`, `PartialEq`, `Eq`.
- Types that need default values implement `Default`.
- No type contains private fields (they are plain data carriers).

## Consumers

- `repo::Repository` — returns these types from its methods.
- `parse` — constructs these types from parsed output.
- `error` — `GitError::MergeConflicts` carries `Vec<String>` (not `GitMergeResult` to keep error simple).
- Downstream crates — pattern-match on and destructure these types.
