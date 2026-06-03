# `parse`

Pure parsers for git porcelain/machine-readable output. No process execution.

## Purpose

This module is the **logic layer** between raw git CLI output and structured Rust types.
It contains only pure functions — no I/O, no async, no side effects. This makes it
trivially testable without a git binary.

## Public API

### Functions

- `parse_status(porcelain: &str) -> Result<GitStatus, GitError>` — parse `status --porcelain`.
- `parse_worktrees(porcelain: &str) -> Result<Vec<GitWorktree>, GitError>` — parse `worktree list --porcelain`.
- `parse_branches(output: &str) -> Result<Vec<String>, GitError>` — parse `branch --format`.
- `parse_merge_tree(output: &str) -> Result<GitMergeResult, GitError>` — parse `merge-tree` output.

## Invariants

- Parsers tolerate extra whitespace and unknown status codes gracefully.
- Parsers do not depend on localized git output.
- Every parser must have unit tests with sample output from real git invocations.

## Consumers

- `repo::Repository` — calls parsers after `GitCommand::run` to convert stdout into structs.
- Unit tests — can exercise parsers in isolation without spawning git.
