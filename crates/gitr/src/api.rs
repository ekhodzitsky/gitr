use crate::error::GitError;
use crate::types::{
    CherryPickOptions, CommitOptions, FetchOptions, GitMergeResult, GitStatus, GitWorktree,
    MergeOptions, Oid, PushOptions, RebaseOptions,
};
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
    async fn head_commit(&self) -> Result<Oid, GitError>;

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

    /// Commit changes with options.
    async fn commit_opts(&self, opts: &CommitOptions<'_>) -> Result<String, GitError>;

    /// Commit changes with `message`.
    ///
    /// If `paths` is empty, commits all changes (`-a`).
    /// When `no_verify` is `true`, `--no-verify` is passed to skip pre-commit
    /// and commit-msg hooks.
    async fn commit(
        &self,
        message: &str,
        paths: &[&std::path::Path],
        no_verify: bool,
    ) -> Result<String, GitError> {
        self.commit_opts(&CommitOptions {
            message,
            paths,
            no_verify,
            amend: false,
            signoff: false,
        })
        .await
    }

    /// Push with options.
    async fn push_opts(&self, opts: &PushOptions<'_>) -> Result<(), GitError>;

    /// Push `branch` to `remote`.
    async fn push(&self, remote: &str, branch: &str, force: bool) -> Result<(), GitError> {
        self.push_opts(&PushOptions {
            remote,
            branch,
            force_with_lease: force,
            ..Default::default()
        })
        .await
    }

    /// Fetch with options.
    async fn fetch_opts(&self, opts: &FetchOptions<'_>) -> Result<(), GitError>;

    /// Fetch from `remote`.
    async fn fetch(&self, remote: &str) -> Result<(), GitError> {
        self.fetch_opts(&FetchOptions {
            remote,
            ..Default::default()
        })
        .await
    }

    /// Read-only merge-tree conflict detection.
    async fn merge_tree(&self, base: &str, branch: &str) -> Result<GitMergeResult, GitError>;

    /// Merge with options.
    async fn merge_opts(&self, opts: &MergeOptions<'_>) -> Result<(), GitError>;

    /// Merge branch into current HEAD.
    async fn merge(&self, branch: &str, no_edit: bool) -> Result<(), GitError> {
        self.merge_opts(&MergeOptions {
            branch,
            no_edit,
            ..Default::default()
        })
        .await
    }

    /// Rebase with options.
    async fn rebase_opts(&self, opts: &RebaseOptions<'_>) -> Result<(), GitError>;

    /// Rebase current HEAD onto branch.
    async fn rebase(&self, branch: &str) -> Result<(), GitError> {
        self.rebase_opts(&RebaseOptions {
            branch,
            ..Default::default()
        })
        .await
    }

    /// Stash changes with an optional message.
    async fn stash(&self, message: Option<&str>) -> Result<(), GitError>;

    /// Get unstaged diff.
    async fn diff(&self) -> Result<String, GitError>;

    /// Get structured unstaged diff.
    async fn diff_structured(&self) -> Result<Vec<crate::types::FileDiff>, GitError>;

    /// Get structured staged diff.
    async fn diff_cached_structured(&self) -> Result<Vec<crate::types::FileDiff>, GitError>;

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

    /// Unset a git config value.
    async fn config_unset(&self, key: &str) -> Result<(), GitError>;

    /// List all tags.
    async fn tag_list(&self) -> Result<Vec<crate::types::GitTag>, GitError>;

    /// Create a new tag.
    async fn tag_create(
        &self,
        name: &str,
        message: Option<&str>,
        force: bool,
    ) -> Result<(), GitError>;

    /// Create a signed tag.
    async fn tag_create_signed(
        &self,
        name: &str,
        message: &str,
        gpg_key: Option<&str>,
    ) -> Result<(), GitError>;

    /// Verify a tag signature.
    async fn verify_tag(&self, name: &str) -> Result<crate::types::GitVerification, GitError>;

    /// Show file contents at a given revision.
    async fn show(&self, path: &str, rev: Option<&str>) -> Result<String, GitError>;

    /// Get blame information for a file.
    async fn blame(&self, path: &str) -> Result<String, GitError>;

    /// Get structured blame information for a file.
    async fn blame_structured(&self, path: &str) -> Result<Vec<crate::types::BlameLine>, GitError>;

    /// Generate patches for a commit range.
    async fn format_patch(&self, range: &str) -> Result<Vec<crate::types::Patch>, GitError>;

    /// Apply a patch string.
    async fn apply_patch(&self, patch: &str, dry_run: bool) -> Result<(), GitError>;

    /// Apply a patch file.
    async fn apply_patch_file(&self, path: &Path, dry_run: bool) -> Result<(), GitError>;

    /// Create a bundle file containing the specified refs.
    async fn bundle_create(&self, output: &Path, refs: Option<&[&str]>) -> Result<(), GitError>;

    /// List refs contained in a bundle file.
    async fn bundle_list_heads(&self, path: &Path) -> Result<Vec<String>, GitError>;

    /// Verify a bundle file is valid and can be applied.
    async fn bundle_verify(&self, path: &Path) -> Result<(), GitError>;

    /// Unbundle (fetch from) a bundle file into the repository.
    async fn bundle_unbundle(&self, path: &Path) -> Result<Vec<String>, GitError>;

    /// List reflog entries for a ref.
    async fn reflog_list(
        &self,
        ref_name: Option<&str>,
    ) -> Result<Vec<crate::types::ReflogEntry>, GitError>;

    /// Expire reflog entries for a ref.
    async fn reflog_expire(
        &self,
        ref_name: &str,
        expire_time: Option<&str>,
    ) -> Result<(), GitError>;

    /// List installed hooks.
    async fn hooks_list(&self) -> Result<Vec<crate::types::Hook>, GitError>;

    /// Install a hook script.
    async fn hook_install(&self, name: &str, script: &str) -> Result<(), GitError>;

    /// Remove a hook.
    async fn hook_remove(&self, name: &str) -> Result<(), GitError>;

    /// Run a hook and capture output.
    async fn run_hook(&self, name: &str) -> Result<crate::types::HookOutput, GitError>;

    /// Check which paths are ignored by git.
    async fn check_ignore(&self, paths: &[&std::path::Path]) -> Result<Vec<String>, GitError>;

    /// Check git attributes for given paths.
    async fn check_attr(
        &self,
        paths: &[&std::path::Path],
        attrs: &[&str],
    ) -> Result<Vec<crate::types::GitAttr>, GitError>;

    /// Start a bisect session.
    async fn bisect_start(
        &self,
        bad: Option<&str>,
        good: &[&str],
    ) -> Result<crate::types::BisectState, GitError>;

    /// Mark a commit as bad during bisect.
    async fn bisect_bad(&self, commit: Option<&str>)
        -> Result<crate::types::BisectState, GitError>;

    /// Mark a commit as good during bisect.
    async fn bisect_good(
        &self,
        commit: Option<&str>,
    ) -> Result<crate::types::BisectState, GitError>;

    /// Reset an active bisect session.
    async fn bisect_reset(&self) -> Result<(), GitError>;

    /// Run a command automatically during bisect.
    async fn bisect_run(&self, command: &str) -> Result<String, GitError>;

    /// List git notes.
    async fn notes_list(
        &self,
        namespace: Option<&str>,
    ) -> Result<Vec<crate::types::GitNote>, GitError>;

    /// Show the note for a given object.
    async fn notes_show(&self, object: &str, namespace: Option<&str>) -> Result<String, GitError>;

    /// Add a note to an object.
    async fn notes_add(
        &self,
        message: &str,
        object: &str,
        namespace: Option<&str>,
        force: bool,
    ) -> Result<(), GitError>;

    /// Remove the note from an object.
    async fn notes_remove(&self, object: &str, namespace: Option<&str>) -> Result<(), GitError>;

    /// Reset the index and working tree.
    async fn reset(
        &self,
        mode: crate::types::ResetMode,
        target: Option<&str>,
    ) -> Result<(), GitError>;

    /// List stash entries.
    async fn stash_list(&self) -> Result<Vec<crate::types::GitStash>, GitError>;

    /// Cherry-pick with options.
    async fn cherry_pick_opts(&self, opts: &CherryPickOptions<'_>) -> Result<(), GitError>;

    /// Cherry-pick one or more commits.
    async fn cherry_pick(&self, commits: &[&str]) -> Result<(), GitError> {
        self.cherry_pick_opts(&CherryPickOptions {
            commits,
            ..Default::default()
        })
        .await
    }

    /// Create a GPG-signed commit.
    async fn commit_signed(
        &self,
        paths: &[&Path],
        message: &str,
        gpg_key: Option<&str>,
        no_verify: bool,
    ) -> Result<(), GitError>;

    /// Verify the GPG signature of a commit.
    async fn verify_commit(&self, sha: &str) -> Result<crate::types::GitVerification, GitError>;

    /// List submodules and their current state.
    async fn submodule_list(&self) -> Result<Vec<crate::types::GitSubmodule>, GitError>;

    /// Add a new submodule.
    async fn submodule_add(&self, url: &str, path: &Path) -> Result<(), GitError>;

    /// Update submodules.
    async fn submodule_update(&self, init: bool, recursive: bool) -> Result<(), GitError>;

    /// Deinitialize a submodule.
    async fn submodule_deinit(&self, path: &Path, force: bool) -> Result<(), GitError>;

    /// Synchronize submodule remote URLs.
    async fn submodule_sync(&self) -> Result<(), GitError>;

    /// List tracked files matching optional filters.
    async fn ls_files(
        &self,
        deleted: bool,
        others: bool,
        exclude_standard: bool,
    ) -> Result<Vec<String>, GitError>;

    /// Get the staged (cached) diff.
    async fn diff_cached(&self) -> Result<String, GitError>;

    /// Create an archive of `ref_name` at `output`.
    async fn archive(&self, ref_name: &str, output: &Path) -> Result<(), GitError>;

    /// Run `git grep` for `pattern`.
    async fn grep(&self, pattern: &str) -> Result<Vec<crate::types::GitGrepResult>, GitError>;

    /// Describe the current commit in terms of the nearest tag.
    async fn describe(&self, tags: bool, long: bool) -> Result<String, GitError>;

    /// Remove untracked files from the working tree.
    async fn clean(
        &self,
        force: bool,
        directories: bool,
        dry_run: bool,
    ) -> Result<Vec<String>, GitError>;

    // Git LFS

    /// Start tracking file paths matching `patterns` with Git LFS.
    async fn lfs_track(&self, patterns: &[&str]) -> Result<(), GitError>;

    /// Stop tracking file paths matching `patterns` with Git LFS.
    async fn lfs_untrack(&self, patterns: &[&str]) -> Result<(), GitError>;

    /// List files tracked by Git LFS.
    async fn lfs_ls_files(&self) -> Result<Vec<crate::types::GitLfsFile>, GitError>;

    /// Lock a file path with Git LFS.
    async fn lfs_lock(&self, path: &str) -> Result<(), GitError>;

    /// Unlock a file path with Git LFS.
    async fn lfs_unlock(&self, path: &str, force: bool) -> Result<(), GitError>;

    // Sparse checkout

    /// Initialize sparse checkout.
    async fn sparse_checkout_init(&self, cone: bool) -> Result<(), GitError>;

    /// Set the sparse-checkout paths, replacing the existing list.
    async fn sparse_checkout_set(&self, paths: &[&str]) -> Result<(), GitError>;

    /// Add paths to the sparse-checkout list.
    async fn sparse_checkout_add(&self, paths: &[&str]) -> Result<(), GitError>;

    /// Disable sparse checkout.
    async fn sparse_checkout_disable(&self) -> Result<(), GitError>;

    /// List the current sparse-checkout paths.
    async fn sparse_checkout_list(&self) -> Result<Vec<String>, GitError>;

    // Missing core operations

    /// Pull from a remote branch.
    async fn pull(&self, remote: &str, branch: &str, rebase: bool) -> Result<(), GitError>;

    /// Switch to a branch.
    async fn switch(&self, branch: &str, create: bool) -> Result<(), GitError>;

    /// Restore working tree files.
    async fn restore(
        &self,
        paths: &[&std::path::Path],
        staged: bool,
        source: Option<&str>,
    ) -> Result<(), GitError>;

    /// Revert one or more commits.
    async fn revert(&self, commits: &[&str], no_edit: bool) -> Result<(), GitError>;

    /// Drop a stash entry.
    async fn stash_drop(&self, index: Option<usize>) -> Result<(), GitError>;

    /// Apply a stash entry without removing it.
    async fn stash_apply(&self, index: Option<usize>) -> Result<(), GitError>;

    /// Show the diff of a stash entry.
    async fn stash_show(&self, index: Option<usize>) -> Result<String, GitError>;

    /// Add a new remote.
    async fn remote_add(&self, name: &str, url: &str) -> Result<(), GitError>;

    /// Remove a remote.
    async fn remote_remove(&self, name: &str) -> Result<(), GitError>;

    /// Rename a remote.
    async fn remote_rename(&self, old: &str, new: &str) -> Result<(), GitError>;

    /// List refs on a remote without fetching.
    async fn ls_remote(
        &self,
        remote: &str,
        refs: Option<&[&str]>,
    ) -> Result<Vec<(String, String)>, GitError>;

    /// Rename a branch.
    async fn branch_rename(&self, old: &str, new: &str, force: bool) -> Result<(), GitError>;

    /// Delete a tag.
    async fn tag_delete(&self, name: &str) -> Result<(), GitError>;

    /// Prune stale worktrees.
    async fn worktree_prune(&self) -> Result<(), GitError>;

    /// Lock a worktree.
    async fn worktree_lock(&self, path: &std::path::Path) -> Result<(), GitError>;

    /// Unlock a worktree.
    async fn worktree_unlock(&self, path: &std::path::Path) -> Result<(), GitError>;

    /// Move a worktree.
    async fn worktree_move(
        &self,
        old_path: &std::path::Path,
        new_path: &std::path::Path,
    ) -> Result<(), GitError>;

    /// Move or rename a tracked file.
    async fn mv(&self, source: &std::path::Path, dest: &std::path::Path) -> Result<(), GitError>;

    /// Remove tracked files.
    async fn rm(&self, paths: &[&std::path::Path], cached: bool) -> Result<(), GitError>;

    /// Find the best common ancestor(s) between commits.
    async fn merge_base(&self, commits: &[&str]) -> Result<String, GitError>;

    /// Resolve a revision to a full SHA.
    async fn rev_parse(&self, rev: &str) -> Result<String, GitError>;

    // Object DB

    /// Compute the OID of `data` without writing it.
    async fn hash_object(&self, data: &[u8]) -> Result<Oid, GitError>;

    /// Write a blob and return its OID.
    async fn write_blob(&self, data: &[u8]) -> Result<Oid, GitError>;

    /// Create a tree from entries and return its OID.
    async fn mktree(&self, entries: &[crate::types::TreeEntry]) -> Result<Oid, GitError>;

    /// Write a tree and return its OID.
    async fn write_tree(&self, entries: &[crate::types::TreeEntry]) -> Result<Oid, GitError>;

    /// Read an object by OID.
    async fn read_object(&self, oid: &Oid) -> Result<crate::types::ObjectContent, GitError>;

    /// Read a tree by OID.
    async fn read_tree(&self, oid: &Oid) -> Result<Vec<crate::types::TreeEntry>, GitError>;

    /// Read a blob by OID.
    async fn read_blob(&self, oid: &Oid) -> Result<Vec<u8>, GitError>;

    /// Read a commit by OID.
    async fn read_commit(&self, oid: &Oid) -> Result<crate::types::GitCommit, GitError>;

    /// Write a commit and return its OID.
    async fn write_commit(
        &self,
        parents: &[&Oid],
        tree: &Oid,
        message: &str,
    ) -> Result<Oid, GitError>;

    /// Read the index.
    async fn read_index(&self) -> Result<Vec<crate::types::IndexEntry>, GitError>;

    /// Update the index for a path.
    async fn update_index(
        &self,
        path: &std::path::Path,
        oid: &Oid,
        mode: u32,
    ) -> Result<(), GitError>;

    /// Remove a path from the index.
    async fn remove_index(&self, path: &std::path::Path) -> Result<(), GitError>;
}
