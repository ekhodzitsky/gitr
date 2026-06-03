use crate::error::GitError;
use crate::types::{GitMergeResult, GitStatus, GitWorktree};
use async_trait::async_trait;
use std::path::Path;

/// High-level async trait for git repository operations.
///
/// Implemented by [`Repository`](crate::Repository). Downstream consumers can
/// use `Box<dyn GitApi>` or `Arc<dyn GitApi>` for mockability in tests.
#[async_trait]
pub trait GitApi {
    /// Ensure the working tree is clean (no staged/unstaged changes).
    async fn ensure_clean(&self) -> Result<(), GitError>;

    /// Get structured status.
    async fn status(&self) -> Result<GitStatus, GitError>;

    /// Get the current branch name.
    async fn current_branch(&self) -> Result<String, GitError>;

    /// Get the short SHA of HEAD.
    async fn head_commit(&self) -> Result<String, GitError>;

    /// List changed files (modified, staged, untracked).
    async fn changed_files(&self) -> Result<Vec<String>, GitError>;

    /// Add a worktree at `path` tracking `branch`.
    async fn worktree_add(&self, path: &Path, branch: &str) -> Result<GitWorktree, GitError>;

    /// Remove a worktree at `path`.
    async fn worktree_remove(&self, path: &Path, force: bool) -> Result<(), GitError>;

    /// List all worktrees.
    async fn worktree_list(&self) -> Result<Vec<GitWorktree>, GitError>;

    /// Create a new branch.
    async fn branch_create(&self, name: &str, start_point: Option<&str>) -> Result<(), GitError>;

    /// Delete a local branch.
    async fn branch_delete(&self, name: &str, force: bool) -> Result<(), GitError>;

    /// Check whether a branch exists.
    async fn branch_exists(&self, name: &str) -> Result<bool, GitError>;

    /// Checkout a branch.
    async fn checkout(&self, branch: &str) -> Result<(), GitError>;

    /// Commit changes with `message`.
    ///
    /// If `paths` is empty, commits all changes (`-a`).
    async fn commit(&self, message: &str, paths: &[&std::path::Path]) -> Result<String, GitError>;

    /// Push `branch` to `remote`.
    async fn push(&self, remote: &str, branch: &str, force: bool) -> Result<(), GitError>;

    /// Fetch from `remote`.
    async fn fetch(&self, remote: &str) -> Result<(), GitError>;

    /// Read-only merge-tree conflict detection.
    async fn merge_tree(&self, base: &str, branch: &str) -> Result<GitMergeResult, GitError>;

    /// Rebase current HEAD onto branch.
    async fn rebase(&self, branch: &str) -> Result<(), GitError>;

    /// Stash changes with an optional message.
    async fn stash(&self, message: Option<&str>) -> Result<(), GitError>;

    /// Get unstaged diff.
    async fn diff(&self) -> Result<String, GitError>;

    /// Get the commit log.
    async fn log(
        &self,
        max_count: Option<usize>,
    ) -> Result<Vec<crate::types::GitLogEntry>, GitError>;

    /// Get a paginated slice of the commit log.
    async fn log_paginated(
        &self,
        skip: usize,
        max_count: usize,
    ) -> Result<Vec<crate::types::GitLogEntry>, GitError>;

    /// List configured remotes.
    async fn remotes(&self) -> Result<Vec<crate::types::GitRemote>, GitError>;

    /// Read a git config value.
    async fn config_get(&self, key: &str) -> Result<Option<String>, GitError>;

    /// Set a git config value.
    async fn config_set(&self, key: &str, value: &str) -> Result<(), GitError>;

    /// List all tags.
    async fn tag_list(&self) -> Result<Vec<crate::types::GitTag>, GitError>;

    /// Create a new tag.
    async fn tag_create(
        &self,
        name: &str,
        message: Option<&str>,
        force: bool,
    ) -> Result<(), GitError>;

    /// Show file contents at a given revision.
    async fn show(&self, path: &str, rev: Option<&str>) -> Result<String, GitError>;

    /// Get blame information for a file.
    async fn blame(&self, path: &str) -> Result<String, GitError>;

    /// Reset the index and working tree.
    async fn reset(
        &self,
        mode: crate::types::ResetMode,
        target: Option<&str>,
    ) -> Result<(), GitError>;

    /// List stash entries.
    async fn stash_list(&self) -> Result<Vec<crate::types::GitStash>, GitError>;

    /// Cherry-pick one or more commits.
    async fn cherry_pick(&self, commits: &[&str]) -> Result<(), GitError>;
}
