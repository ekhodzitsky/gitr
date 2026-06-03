use crate::api::GitApi;
use crate::command::GitCommand;
use crate::error::GitError;
use crate::parse;
use crate::types::{GitMergeResult, GitStatus, GitWorktree};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
#[cfg(feature = "tracing")]
use tracing::debug;

#[cfg(test)]
mod tests;

/// A typed handle to a git repository.
#[derive(Debug, Clone)]
pub struct Repository {
    root: PathBuf,
    cmd: GitCommand,
}

impl Repository {
    /// Open a repository, validating that `.git` exists and git is in PATH.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let root = tokio::fs::canonicalize(path.as_ref())
            .await
            .map_err(|e| GitError::Io(format!("failed to canonicalize path: {e}")))?;
        let dot_git = root.join(".git");
        if !dot_git.exists() {
            return Err(GitError::NotARepo(root));
        }
        let cmd = GitCommand::new(root.clone())?;
        Ok(Self { root, cmd })
    }

    /// Open an existing worktree directory as a repository handle.
    ///
    /// This is currently an alias for [`Repository::open`](Self::open).
    /// It does not verify that the directory is actually a git worktree.
    pub async fn open_worktree(path: impl AsRef<Path>) -> Result<Self, GitError> {
        Self::open(path).await
    }

    /// Path to the repository root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Ensure the working tree is clean (no staged/unstaged changes; untracked ignored).
    pub async fn ensure_clean(&self) -> Result<(), GitError> {
        let out = self.cmd.run(&["status", "--porcelain"]).await?;
        let status = parse::parse_status(&out.stdout)?;
        if !status.staged.is_empty() || !status.unstaged.is_empty() {
            let mut files: Vec<String> = status.staged;
            files.extend(status.unstaged);
            return Err(GitError::Dirty(files.join(", ")));
        }
        Ok(())
    }

    /// Get the current branch name.
    pub async fn current_branch(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["rev-parse", "--abbrev-ref", "HEAD"]).await?;
        let branch = out.stdout.trim().to_string();
        if branch.is_empty() || branch == "HEAD" {
            return Err(GitError::Parse("detached HEAD or empty branch".to_string()));
        }
        Ok(branch)
    }

    /// Get the short SHA of HEAD.
    pub async fn head_commit(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["rev-parse", "--short", "HEAD"]).await?;
        Ok(out.stdout.trim().to_string())
    }

    /// Get the full SHA of HEAD.
    pub async fn head_commit_full(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["rev-parse", "HEAD"]).await?;
        Ok(out.stdout.trim().to_string())
    }

    /// List changed files (modified, staged, untracked).
    pub async fn changed_files(&self) -> Result<Vec<String>, GitError> {
        let out = self.cmd.run(&["status", "--porcelain"]).await?;
        let status = parse::parse_status(&out.stdout)?;
        let mut files = Vec::new();
        files.extend(status.staged);
        files.extend(status.unstaged);
        files.extend(status.untracked);
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// List untracked files.
    pub async fn untracked_files(&self) -> Result<Vec<String>, GitError> {
        let out = self.cmd.run(&["status", "--porcelain"]).await?;
        let status = parse::parse_status(&out.stdout)?;
        Ok(status.untracked)
    }

    /// List files with unresolved merge/rebase conflicts.
    pub async fn conflicted_files(&self) -> Result<Vec<String>, GitError> {
        let out = self
            .cmd
            .run(&["diff", "--name-only", "--diff-filter=U"])
            .await?;
        let files: Vec<String> = out.stdout.lines().map(|s| s.to_string()).collect();
        Ok(files)
    }

    /// Whether the repository has unresolved merge/rebase conflicts.
    pub async fn is_merge_conflict(&self) -> Result<bool, GitError> {
        Ok(!self.conflicted_files().await?.is_empty())
    }

    /// Whether there are no staged or unstaged changes (untracked ignored).
    pub async fn is_nothing_to_commit(&self) -> Result<bool, GitError> {
        let files = self.changed_files().await?;
        Ok(files.is_empty())
    }

    /// Whether there are untracked files.
    pub async fn has_untracked_files(&self) -> Result<bool, GitError> {
        Ok(!self.untracked_files().await?.is_empty())
    }

    /// Raw `git status --porcelain` output.
    pub async fn status_porcelain(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["status", "--porcelain"]).await?;
        Ok(out.stdout.to_string())
    }

    /// Parse and return structured status.
    pub async fn status(&self) -> Result<GitStatus, GitError> {
        let out = self.cmd.run(&["status", "--porcelain"]).await?;
        parse::parse_status(&out.stdout)
    }

    /// Parse and return structured status from null-delimited porcelain.
    pub async fn status_z(&self) -> Result<GitStatus, GitError> {
        let out = self.cmd.run(&["status", "--porcelain", "-z"]).await?;
        parse::parse_status_z(&out.stdout)
    }

    /// Add a worktree at `path` tracking `branch`.
    pub async fn worktree_add(
        &self,
        path: impl AsRef<Path>,
        branch: &str,
    ) -> Result<GitWorktree, GitError> {
        let path = path.as_ref();
        let out = self
            .cmd
            .run(&["worktree", "add", &path.to_string_lossy(), branch])
            .await;

        if let Err(GitError::CommandFailed { ref stderr, .. }) = out {
            if stderr.contains("already exists") || stderr.contains("is already registered") {
                return Err(GitError::WorktreeExists(path.to_string_lossy().to_string()));
            }
        }
        out?;
        Ok(GitWorktree {
            path: path.to_path_buf(),
            branch: branch.to_string(),
        })
    }

    /// Remove a worktree at `path`.
    pub async fn worktree_remove(
        &self,
        path: impl AsRef<Path>,
        force: bool,
    ) -> Result<(), GitError> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy();
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&path_str);
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// List all worktrees.
    pub async fn worktree_list(&self) -> Result<Vec<GitWorktree>, GitError> {
        let out = self.cmd.run(&["worktree", "list", "--porcelain"]).await?;
        parse::parse_worktrees(&out.stdout)
    }

    /// Create a new branch.
    pub async fn branch_create(
        &self,
        name: &str,
        start_point: Option<&str>,
    ) -> Result<(), GitError> {
        let mut args = vec!["branch", name];
        if let Some(sp) = start_point {
            args.push(sp);
        }
        let out = self.cmd.run(&args).await;
        if let Err(GitError::CommandFailed { ref stderr, .. }) = out {
            if stderr.contains("already exists") {
                return Err(GitError::BranchExists(name.to_string()));
            }
        }
        out?;
        Ok(())
    }

    /// Delete a local branch.
    pub async fn branch_delete(&self, name: &str, force: bool) -> Result<(), GitError> {
        let flag = if force { "-D" } else { "-d" };
        let out = self.cmd.run(&["branch", flag, name]).await;
        if let Err(GitError::CommandFailed { ref stderr, .. }) = out {
            if stderr.contains("not found") {
                return Err(GitError::BranchNotFound(name.to_string()));
            }
        }
        out?;
        Ok(())
    }

    /// Check whether a branch exists (local or remote-tracking).
    pub async fn branch_exists(&self, name: &str) -> Result<bool, GitError> {
        let out = self
            .cmd
            .run(&["branch", "--format=%(refname:short)"])
            .await?;
        let branches = parse::parse_branches(&out.stdout)?;
        Ok(branches.iter().any(|b| b == name))
    }

    /// Checkout a branch.
    pub async fn checkout(&self, branch: &str) -> Result<(), GitError> {
        let out = self.cmd.run(&["checkout", branch]).await;
        if let Err(GitError::CommandFailed { ref stderr, .. }) = out {
            if stderr.contains("did not match") || stderr.contains("not found") {
                return Err(GitError::BranchNotFound(branch.to_string()));
            }
        }
        out?;
        Ok(())
    }

    /// Read-only merge-tree conflict detection.
    pub async fn merge_tree(&self, base: &str, branch: &str) -> Result<GitMergeResult, GitError> {
        let out = self.cmd.run(&["merge-tree", base, branch]).await;
        match out {
            Ok(o) => parse::parse_merge_tree(&o.stdout),
            Err(GitError::CommandFailed {
                stdout,
                stderr,
                exit_code,
                command,
            }) => {
                let combined = format!("{stdout}\n{stderr}");
                let result = parse::parse_merge_tree(&combined)?;
                if result.has_conflicts {
                    #[cfg(feature = "tracing")]
                    debug!(
                        base,
                        branch,
                        files = ?result.conflict_files,
                        "merge-tree detected conflicts"
                    );
                    Ok(result)
                } else {
                    Err(GitError::CommandFailed {
                        command,
                        exit_code,
                        stderr,
                        stdout,
                    })
                }
            }
            Err(other) => Err(other),
        }
    }

    /// Commit with `message`. If `paths` is empty, commits all changes (`-a`).
    pub async fn commit(
        &self,
        message: &str,
        paths: &[impl AsRef<Path>],
    ) -> Result<String, GitError> {
        let mut args: Vec<String> = vec!["commit".into(), "-m".into(), message.into()];
        if paths.is_empty() {
            args.push("-a".into());
        } else {
            args.push("--".into());
            for p in paths {
                args.push(p.as_ref().to_string_lossy().into());
            }
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let _out = self.cmd.run(&args_ref).await?;
        let sha = self.head_commit().await?;
        #[cfg(feature = "tracing")]
        debug!(%sha, "committed");
        Ok(sha)
    }

    /// Push `branch` to `remote`.
    pub async fn push(&self, remote: &str, branch: &str, force: bool) -> Result<(), GitError> {
        let mut args = vec!["push", remote, branch];
        if force {
            args.push("--force-with-lease");
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// Push with `--force` (not `--force-with-lease`).
    pub async fn push_force(&self, remote: &str, branch: &str) -> Result<(), GitError> {
        self.cmd.run(&["push", "--force", remote, branch]).await?;
        Ok(())
    }

    /// Fetch from `remote`.
    pub async fn fetch(&self, remote: &str) -> Result<(), GitError> {
        self.cmd.run(&["fetch", remote]).await?;
        Ok(())
    }

    /// Get the URL for `remote`, if configured.
    pub async fn remote_url(&self, remote: &str) -> Result<Option<String>, GitError> {
        let out = self.cmd.run(&["remote", "get-url", remote]).await;
        match out {
            Ok(o) => Ok(Some(o.stdout.trim().to_string())),
            Err(GitError::CommandFailed { stderr, .. }) if stderr.contains("No such remote") => {
                Ok(None)
            }
            Err(other) => Err(other),
        }
    }

    /// Get unstaged diff.
    pub async fn diff(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["diff"]).await?;
        Ok(out.stdout.to_string())
    }

    /// Get diff statistics via `--shortstat`.
    pub async fn diff_shortstat(&self) -> Result<crate::parse::DiffShortstat, GitError> {
        let out = self.cmd.run(&["diff", "--shortstat"]).await?;
        crate::parse::parse_diff_shortstat(&out.stdout)
    }

    /// Get diff for specific paths.
    pub async fn diff_files(&self, paths: &[impl AsRef<Path>]) -> Result<String, GitError> {
        let mut args: Vec<String> = vec!["diff".into(), "--".into()];
        for p in paths {
            args.push(p.as_ref().to_string_lossy().into_owned());
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let out = self.cmd.run(&args_ref).await?;
        Ok(out.stdout.to_string())
    }

    /// Stage all changes (including untracked).
    pub async fn add_all(&self) -> Result<(), GitError> {
        self.cmd.run(&["add", "-A"]).await?;
        Ok(())
    }

    /// Stage a specific path.
    pub async fn add(&self, path: impl AsRef<Path>) -> Result<(), GitError> {
        let path_str = path.as_ref().to_string_lossy();
        self.cmd.run(&["add", &path_str]).await?;
        Ok(())
    }

    /// Stash changes with an optional message.
    pub async fn stash(&self, message: Option<&str>) -> Result<(), GitError> {
        let mut args = vec!["stash", "push"];
        if let Some(msg) = message {
            args.push("-m");
            args.push(msg);
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// Pop the latest stash.
    pub async fn stash_pop(&self) -> Result<(), GitError> {
        self.cmd.run(&["stash", "pop"]).await?;
        Ok(())
    }

    /// Merge branch into current HEAD (mutating).
    pub async fn merge(&self, branch: &str, no_edit: bool) -> Result<(), GitError> {
        let mut args = vec!["merge", branch];
        if no_edit {
            args.push("--no-edit");
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// Rebase current HEAD onto branch.
    pub async fn rebase(&self, branch: &str) -> Result<(), GitError> {
        self.cmd.run(&["rebase", branch]).await?;
        Ok(())
    }

    /// Abort an in-progress rebase.
    pub async fn rebase_abort(&self) -> Result<(), GitError> {
        self.cmd.run(&["rebase", "--abort"]).await?;
        Ok(())
    }

    /// Continue an in-progress rebase after conflicts are resolved.
    pub async fn rebase_continue(&self) -> Result<(), GitError> {
        self.cmd
            .run_with_env(&["rebase", "--continue"], &[("GIT_EDITOR", "true")])
            .await?;
        Ok(())
    }

    /// Get the commit log.
    pub async fn log(
        &self,
        max_count: Option<usize>,
    ) -> Result<Vec<crate::types::GitLogEntry>, GitError> {
        let mut args: Vec<String> = vec!["log".into(), "--format=%H|%s|%an|%at".into()];
        if let Some(n) = max_count {
            args.push("-n".into());
            args.push(n.to_string());
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let out = self.cmd.run(&args_ref).await?;
        parse::parse_log(&out.stdout)
    }

    /// Get a paginated slice of the commit log.
    ///
    /// Use `skip` to offset and `max_count` to limit. Useful for large
    /// repositories where loading the entire log into memory is impractical.
    pub async fn log_paginated(
        &self,
        skip: usize,
        max_count: usize,
    ) -> Result<Vec<crate::types::GitLogEntry>, GitError> {
        let args_ref: Vec<String> = vec![
            "log".into(),
            "--format=%H|%s|%an|%at".into(),
            "--skip".into(),
            skip.to_string(),
            "-n".into(),
            max_count.to_string(),
        ];
        let args_str: Vec<&str> = args_ref.iter().map(|s| s.as_str()).collect();
        let out = self.cmd.run(&args_str).await?;
        parse::parse_log(&out.stdout)
    }

    /// List configured remotes.
    pub async fn remotes(&self) -> Result<Vec<crate::types::GitRemote>, GitError> {
        let out = self.cmd.run(&["remote", "-v"]).await?;
        parse::parse_remotes(&out.stdout)
    }

    /// Get the default branch name from remote.
    pub async fn default_branch(&self) -> Result<String, GitError> {
        let out = self
            .cmd
            .run(&["symbolic-ref", "refs/remotes/origin/HEAD"])
            .await?;
        let stdout = out.stdout.trim();
        if let Some(branch) = stdout.strip_prefix("refs/remotes/origin/") {
            if !branch.is_empty() {
                return Ok(branch.to_string());
            }
        }
        Err(GitError::Parse(format!(
            "unexpected origin/HEAD format: {stdout}"
        )))
    }

    /// Read a git config value.
    pub async fn config_get(&self, key: &str) -> Result<Option<String>, GitError> {
        let out = self.cmd.run(&["config", key]).await;
        match out {
            Ok(o) => Ok(Some(o.stdout.trim().to_string())),
            Err(GitError::CommandFailed { ref stderr, .. })
                if stderr.contains("not in config") || stderr.contains("has no value") =>
            {
                Ok(None)
            }
            Err(other) => Err(other),
        }
    }

    /// Set a git config value.
    pub async fn config_set(&self, key: &str, value: &str) -> Result<(), GitError> {
        self.cmd.run(&["config", key, value]).await?;
        Ok(())
    }

    /// List all tags.
    pub async fn tag_list(&self) -> Result<Vec<crate::types::GitTag>, GitError> {
        let out = self
            .cmd
            .run(&[
                "tag",
                "--list",
                "--format=%(refname:short)|%(objectname:short)|%(subject)",
            ])
            .await?;
        let mut tags = Vec::new();
        for line in out.stdout.lines() {
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() == 3 {
                tags.push(crate::types::GitTag {
                    name: parts[0].to_string(),
                    sha: parts[1].to_string(),
                    message: parts[2].to_string(),
                });
            }
        }
        Ok(tags)
    }

    /// Create a new tag.
    pub async fn tag_create(
        &self,
        name: &str,
        message: Option<&str>,
        force: bool,
    ) -> Result<(), GitError> {
        let mut args: Vec<String> = vec!["tag".into()];
        if force {
            args.push("-f".into());
        }
        if let Some(msg) = message {
            args.push("-a".into());
            args.push("-m".into());
            args.push(msg.into());
        }
        args.push(name.into());
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd.run(&args_ref).await?;
        Ok(())
    }

    /// Show file contents at a given revision.
    pub async fn show(&self, path: &str, rev: Option<&str>) -> Result<String, GitError> {
        let spec = match rev {
            Some(r) => format!("{r}:{path}"),
            None => format!("HEAD:{path}"),
        };
        let out = self.cmd.run(&["show", &spec]).await?;
        Ok(out.stdout)
    }

    /// Get blame information for a file.
    pub async fn blame(&self, path: &str) -> Result<String, GitError> {
        let out = self.cmd.run(&["blame", "--line-porcelain", path]).await?;
        Ok(out.stdout)
    }

    /// Reset the index and working tree.
    pub async fn reset(
        &self,
        mode: crate::types::ResetMode,
        target: Option<&str>,
    ) -> Result<(), GitError> {
        let mode_flag = match mode {
            crate::types::ResetMode::Soft => "--soft",
            crate::types::ResetMode::Mixed => "--mixed",
            crate::types::ResetMode::Hard => "--hard",
        };
        let mut args = vec!["reset", mode_flag];
        if let Some(t) = target {
            args.push(t);
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// List stash entries.
    pub async fn stash_list(&self) -> Result<Vec<crate::types::GitStash>, GitError> {
        let out = self
            .cmd
            .run(&["stash", "list", "--format=%H|%gd|%s"])
            .await?;
        let mut stashes = Vec::new();
        for line in out.stdout.lines() {
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() == 3 {
                stashes.push(crate::types::GitStash {
                    sha: parts[0].to_string(),
                    ref_name: parts[1].to_string(),
                    message: parts[2].to_string(),
                });
            }
        }
        Ok(stashes)
    }

    /// Cherry-pick one or more commits.
    pub async fn cherry_pick(&self, commits: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["cherry-pick"];
        for c in commits {
            args.push(c);
        }
        self.cmd.run(&args).await?;
        Ok(())
    }
}

#[async_trait]
impl GitApi for Repository {
    async fn ensure_clean(&self) -> Result<(), GitError> {
        self.ensure_clean().await
    }

    async fn status(&self) -> Result<GitStatus, GitError> {
        self.status().await
    }

    async fn current_branch(&self) -> Result<String, GitError> {
        self.current_branch().await
    }

    async fn head_commit(&self) -> Result<String, GitError> {
        self.head_commit().await
    }

    async fn changed_files(&self) -> Result<Vec<String>, GitError> {
        self.changed_files().await
    }

    async fn worktree_add(&self, path: &Path, branch: &str) -> Result<GitWorktree, GitError> {
        self.worktree_add(path, branch).await
    }

    async fn worktree_remove(&self, path: &Path, force: bool) -> Result<(), GitError> {
        self.worktree_remove(path, force).await
    }

    async fn worktree_list(&self) -> Result<Vec<GitWorktree>, GitError> {
        self.worktree_list().await
    }

    async fn branch_create(&self, name: &str, start_point: Option<&str>) -> Result<(), GitError> {
        self.branch_create(name, start_point).await
    }

    async fn branch_delete(&self, name: &str, force: bool) -> Result<(), GitError> {
        self.branch_delete(name, force).await
    }

    async fn branch_exists(&self, name: &str) -> Result<bool, GitError> {
        self.branch_exists(name).await
    }

    async fn checkout(&self, branch: &str) -> Result<(), GitError> {
        self.checkout(branch).await
    }

    async fn commit(&self, message: &str, paths: &[&Path]) -> Result<String, GitError> {
        self.commit(message, paths).await
    }

    async fn push(&self, remote: &str, branch: &str, force: bool) -> Result<(), GitError> {
        self.push(remote, branch, force).await
    }

    async fn fetch(&self, remote: &str) -> Result<(), GitError> {
        self.fetch(remote).await
    }

    async fn merge_tree(&self, base: &str, branch: &str) -> Result<GitMergeResult, GitError> {
        self.merge_tree(base, branch).await
    }

    async fn rebase(&self, branch: &str) -> Result<(), GitError> {
        self.rebase(branch).await
    }

    async fn stash(&self, message: Option<&str>) -> Result<(), GitError> {
        self.stash(message).await
    }

    async fn diff(&self) -> Result<String, GitError> {
        self.diff().await
    }

    async fn log(
        &self,
        max_count: Option<usize>,
    ) -> Result<Vec<crate::types::GitLogEntry>, GitError> {
        self.log(max_count).await
    }

    async fn log_paginated(
        &self,
        skip: usize,
        max_count: usize,
    ) -> Result<Vec<crate::types::GitLogEntry>, GitError> {
        self.log_paginated(skip, max_count).await
    }

    async fn remotes(&self) -> Result<Vec<crate::types::GitRemote>, GitError> {
        self.remotes().await
    }

    async fn config_get(&self, key: &str) -> Result<Option<String>, GitError> {
        self.config_get(key).await
    }

    async fn config_set(&self, key: &str, value: &str) -> Result<(), GitError> {
        self.config_set(key, value).await
    }

    async fn tag_list(&self) -> Result<Vec<crate::types::GitTag>, GitError> {
        self.tag_list().await
    }

    async fn tag_create(
        &self,
        name: &str,
        message: Option<&str>,
        force: bool,
    ) -> Result<(), GitError> {
        self.tag_create(name, message, force).await
    }

    async fn show(&self, path: &str, rev: Option<&str>) -> Result<String, GitError> {
        self.show(path, rev).await
    }

    async fn blame(&self, path: &str) -> Result<String, GitError> {
        self.blame(path).await
    }

    async fn reset(
        &self,
        mode: crate::types::ResetMode,
        target: Option<&str>,
    ) -> Result<(), GitError> {
        self.reset(mode, target).await
    }

    async fn stash_list(&self) -> Result<Vec<crate::types::GitStash>, GitError> {
        self.stash_list().await
    }

    async fn cherry_pick(&self, commits: &[&str]) -> Result<(), GitError> {
        self.cherry_pick(commits).await
    }
}
