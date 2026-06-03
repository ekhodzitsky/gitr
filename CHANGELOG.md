# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
