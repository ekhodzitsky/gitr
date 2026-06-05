use crate::api::GitApi;
use crate::cache::Cache;
use crate::circuit::CircuitBreaker;
use crate::command::GitCommand;
use crate::error::GitError;
use crate::parse;
use crate::types::{
    CherryPickOptions, CommitOptions, FetchOptions, GitLfsFile, GitMergeResult, GitStatus,
    GitVersion, GitWorktree, IndexEntry, MergeOptions, ObjectContent, Oid, PushOptions,
    RebaseOptions, TreeEntry,
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;
#[cfg(feature = "stream")]
use tokio_stream::wrappers::LinesStream;
#[cfg(feature = "stream")]
use tokio_stream::Stream;
#[cfg(feature = "tracing")]
use tracing::debug;

#[cfg(test)]
mod tests;

/// A typed handle to a git repository.
#[derive(Debug, Clone)]
pub struct Repository {
    root: PathBuf,
    cmd: GitCommand,
    cache: Option<Cache>,
}

#[cfg(feature = "stream")]
struct LogStream {
    _child: tokio::process::Child,
    lines: Pin<Box<LinesStream<BufReader<tokio::process::ChildStdout>>>>,
}

#[cfg(feature = "stream")]
impl Stream for LogStream {
    type Item = Result<crate::types::GitLogEntry, GitError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            match this.lines.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(line))) if line.trim().is_empty() => continue,
                Poll::Ready(Some(Ok(line))) => {
                    return Poll::Ready(Some(parse::parse_log_line(&line)));
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(GitError::Io(format!("read error: {e}")))));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(feature = "stream")]
struct GrepStream {
    _child: tokio::process::Child,
    lines: Pin<Box<LinesStream<BufReader<tokio::process::ChildStdout>>>>,
}

#[cfg(feature = "stream")]
impl Stream for GrepStream {
    type Item = Result<crate::types::GitGrepResult, GitError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.lines.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(line))) => {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() != 3 {
                    return Poll::Ready(Some(Err(GitError::Parse(format!(
                        "invalid grep line: {line}"
                    )))));
                }
                let line_num = match parts[1].parse::<u32>() {
                    Ok(n) => n,
                    Err(e) => {
                        return Poll::Ready(Some(Err(GitError::Parse(format!(
                            "invalid grep line number '{line}': {e}"
                        )))));
                    }
                };
                Poll::Ready(Some(Ok(crate::types::GitGrepResult {
                    path: parts[0].to_string(),
                    line: line_num,
                    text: parts[2].to_string(),
                })))
            }
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Some(Err(GitError::Io(format!("read error: {e}")))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(feature = "stream")]
struct LsFilesStream {
    _child: tokio::process::Child,
    lines: Pin<Box<LinesStream<BufReader<tokio::process::ChildStdout>>>>,
}

#[cfg(feature = "stream")]
impl Stream for LsFilesStream {
    type Item = Result<String, GitError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.lines.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(line))) => Poll::Ready(Some(Ok(line))),
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Some(Err(GitError::Io(format!("read error: {e}")))))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(feature = "stream")]
struct BlameStream {
    _child: tokio::process::Child,
    lines: Pin<Box<LinesStream<BufReader<tokio::process::ChildStdout>>>>,
    buf_sha: String,
    buf_author: String,
    buf_author_mail: String,
    buf_author_time: String,
}

#[cfg(feature = "stream")]
impl Stream for BlameStream {
    type Item = Result<crate::types::BlameLine, GitError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            match this.lines.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(line))) => {
                    if let Some(content) = line.strip_prefix('\t') {
                        // Content line — block is complete.
                        let content = content.to_string();
                        let line_no = 0usize; // Will be assigned sequentially downstream
                        let sha = std::mem::take(&mut this.buf_sha);
                        let author = std::mem::take(&mut this.buf_author);
                        let author_mail = std::mem::take(&mut this.buf_author_mail);
                        let author_time = std::mem::take(&mut this.buf_author_time);
                        return Poll::Ready(Some(Ok(crate::types::BlameLine {
                            commit: sha,
                            author,
                            author_mail,
                            author_time,
                            line_no,
                            content,
                        })));
                    } else if let Some(rest) = line.strip_prefix("author ") {
                        this.buf_author = rest.to_string();
                    } else if let Some(rest) = line.strip_prefix("author-mail ") {
                        this.buf_author_mail = rest.trim_matches('<').trim_matches('>').to_string();
                    } else if let Some(rest) = line.strip_prefix("author-time ") {
                        this.buf_author_time = rest.to_string();
                    } else if !line.is_empty()
                        && line.as_bytes()[0].is_ascii_hexdigit()
                        && line.split_whitespace().next().map(|s| s.len()) == Some(40)
                    {
                        // New blame block header — SHA <orig-line> <final-line> <group-lines>
                        this.buf_sha = line.split_whitespace().next().unwrap_or("").to_string();
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(GitError::Io(format!("read error: {e}")))));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
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
        Ok(Self {
            root,
            cmd,
            cache: None,
        })
    }

    /// Open an existing worktree directory as a repository handle.
    ///
    /// This is currently an alias for [`Repository::open`](Self::open).
    /// It does not verify that the directory is actually a git worktree.
    pub async fn open_worktree(path: impl AsRef<Path>) -> Result<Self, GitError> {
        Self::open(path).await
    }

    /// Initialize a new git repository at `path` and return a handle.
    ///
    /// The parent directory must already exist.
    pub async fn init(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let path = path.as_ref();
        let git_bin = crate::command::git_bin_path()?;
        let parent = path.parent().unwrap_or(Path::new("."));
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(".");
        let output = timeout(
            Duration::from_secs(60),
            tokio::process::Command::new(&git_bin)
                .current_dir(parent)
                .args(["init", name])
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_ASKPASS", "echo")
                .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| GitError::Timeout(Duration::from_secs(60), "git init".to_string()))?
        .map_err(|e| GitError::Io(format!("failed to run git init: {e}")))?;
        if !output.status.success() {
            return Err(GitError::CommandFailed {
                command: format!("git init {name}"),
                exit_code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            });
        }
        Self::open(path).await
    }

    /// Clone a remote repository into `path` and return a handle.
    ///
    /// The parent directory must already exist.  The operation is subject to
    /// the default 60-second timeout.
    pub async fn clone(url: &str, path: impl AsRef<Path>) -> Result<Self, GitError> {
        let path = path.as_ref();
        let git_bin = crate::command::git_bin_path()?;
        let parent = path.parent().unwrap_or(Path::new("."));
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("repo");
        let output = timeout(
            Duration::from_secs(60),
            tokio::process::Command::new(&git_bin)
                .current_dir(parent)
                .args(["clone", url, name])
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_ASKPASS", "echo")
                .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| GitError::Timeout(Duration::from_secs(60), "git clone".to_string()))?
        .map_err(|e| GitError::Io(format!("failed to run git clone: {e}")))?;
        if !output.status.success() {
            return Err(GitError::CommandFailed {
                command: format!("git clone {url} {name}"),
                exit_code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            });
        }
        Self::open(path).await
    }

    /// Clone a repository with options.
    pub async fn clone_opts(opts: &crate::types::CloneOptions<'_>) -> Result<Self, GitError> {
        let path = opts.path;
        let git_bin = crate::command::git_bin_path()?;
        let parent = path.parent().unwrap_or(Path::new("."));
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("repo");
        let mut args: Vec<String> = vec!["clone".into(), opts.url.into(), name.into()];
        if let Some(depth) = opts.depth {
            args.push("--depth".into());
            args.push(depth.to_string());
        }
        if let Some(branch) = opts.branch {
            args.push("--branch".into());
            args.push(branch.into());
        }
        if let Some(filter) = opts.filter {
            args.push("--filter".into());
            args.push(filter.into());
        }
        if opts.bare {
            args.push("--bare".into());
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = timeout(
            Duration::from_secs(60),
            tokio::process::Command::new(&git_bin)
                .current_dir(parent)
                .args(&args_ref)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_ASKPASS", "echo")
                .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| GitError::Timeout(Duration::from_secs(60), "git clone".to_string()))?
        .map_err(|e| GitError::Io(format!("failed to run git clone: {e}")))?;
        if !output.status.success() {
            return Err(GitError::CommandFailed {
                command: format!("git clone {}", args_ref.join(" ")),
                exit_code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            });
        }
        Self::open(path).await
    }

    /// Attach a cancellation token to this repository handle.
    ///
    /// When the token is cancelled, any in-flight git command is killed
    /// and returns [`GitError::Io`] with message `"cancelled"`.
    pub fn with_cancel(mut self, cancel: tokio_util::sync::CancellationToken) -> Self {
        self.cmd = self.cmd.with_cancel(cancel);
        self
    }

    /// Override the default 60-second timeout for all git commands issued
    /// through this handle.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.cmd = self.cmd.with_timeout(timeout);
        self
    }

    /// Attach an in-memory cache to this repository handle.
    ///
    /// When caching is enabled, [`status`](Self::status) and related
    /// read-only queries may return cached results. Callers must
    /// invalidate the cache after mutating operations.
    pub fn with_cache(mut self, cache: Cache) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Add a custom environment variable to all subsequent git commands.
    ///
    /// This is useful for authentication scenarios such as setting
    /// `GIT_ASKPASS` or `GIT_SSH_COMMAND`.
    pub fn with_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.cmd = self.cmd.with_env_var(key, value);
        self
    }

    /// Attach a circuit breaker for network operations (fetch, push, pull, clone, ls-remote).
    pub fn with_circuit_breaker(mut self, breaker: CircuitBreaker) -> Self {
        self.cmd = self.cmd.with_circuit_breaker(breaker);
        self
    }

    /// Attach a progress callback for network operations.
    ///
    /// The callback is invoked with each line of stderr as it is produced,
    /// which is useful for reporting clone/fetch/push progress to users.
    pub fn with_progress(mut self, callback: impl Fn(String) + Send + Sync + 'static) -> Self {
        self.cmd = self.cmd.with_progress(callback);
        self
    }

    /// Invalidate the attached cache, if any.
    pub async fn invalidate_cache(&self) {
        if let Some(c) = &self.cache {
            c.invalidate().await;
        }
    }

    /// Path to the repository root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the detected git version for this repository.
    pub fn git_version(&self) -> GitVersion {
        self.cmd.git_version()
    }

    /// Ensure the working tree is clean (no staged/unstaged changes; untracked ignored).
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn ensure_clean(&self) -> Result<(), GitError> {
        let status = self.status().await?;
        if !status.staged.is_empty() || !status.unstaged.is_empty() {
            let mut files: Vec<String> = status.staged;
            files.extend(status.unstaged);
            return Err(GitError::Dirty(files.join(", ")));
        }
        Ok(())
    }

    /// Get the current branch name.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn current_branch(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["rev-parse", "--abbrev-ref", "HEAD"]).await?;
        let branch = out.stdout.trim().to_string();
        if branch.is_empty() || branch == "HEAD" {
            return Err(GitError::Parse("detached HEAD or empty branch".to_string()));
        }
        Ok(branch)
    }

    /// Get the short SHA of HEAD.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn head_commit(&self) -> Result<Oid, GitError> {
        let out = self.cmd.run(&["rev-parse", "--short", "HEAD"]).await?;
        Oid::new(out.stdout.trim())
    }

    /// Get the full SHA of HEAD.
    pub async fn head_commit_full(&self) -> Result<Oid, GitError> {
        let out = self.cmd.run(&["rev-parse", "HEAD"]).await?;
        Oid::new(out.stdout.trim())
    }

    /// List changed files (modified, staged, untracked).
    pub async fn changed_files(&self) -> Result<Vec<String>, GitError> {
        let status = self.status().await?;
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
        let status = self.status().await?;
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
        let status = self.status().await?;
        Ok(status.staged.is_empty() && status.unstaged.is_empty())
    }

    /// Whether there are untracked files.
    pub async fn has_untracked_files(&self) -> Result<bool, GitError> {
        let status = self.status().await?;
        Ok(!status.untracked.is_empty())
    }

    /// Raw `git status --porcelain` output.
    pub async fn status_porcelain(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["status", "--porcelain"]).await?;
        Ok(out.stdout)
    }

    /// Parse and return structured status.
    pub async fn status(&self) -> Result<GitStatus, GitError> {
        if let Some(c) = &self.cache {
            if let Some(cached) = c.get_status().await {
                return Ok(cached);
            }
        }
        let out = self.cmd.run(&["status", "--porcelain"]).await?;
        let status = parse::parse_status(&out.stdout)?;
        if let Some(c) = &self.cache {
            c.set_status(status.clone()).await;
        }
        Ok(status)
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
        self.invalidate_cache().await;
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
        self.invalidate_cache().await;
        Ok(())
    }

    /// List all worktrees.
    pub async fn worktree_list(&self) -> Result<Vec<GitWorktree>, GitError> {
        let out = self.cmd.run(&["worktree", "list", "--porcelain"]).await?;
        parse::parse_worktrees(&out.stdout)
    }

    /// Prune stale worktrees.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn worktree_prune(&self) -> Result<(), GitError> {
        self.cmd.run(&["worktree", "prune"]).await?;
        Ok(())
    }

    /// Lock a worktree to prevent pruning.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    pub async fn worktree_lock(&self, path: impl AsRef<Path>) -> Result<(), GitError> {
        let path_str = path.as_ref().to_string_lossy();
        self.cmd.run(&["worktree", "lock", &path_str]).await?;
        Ok(())
    }

    /// Unlock a worktree.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    pub async fn worktree_unlock(&self, path: impl AsRef<Path>) -> Result<(), GitError> {
        let path_str = path.as_ref().to_string_lossy();
        self.cmd.run(&["worktree", "unlock", &path_str]).await?;
        Ok(())
    }

    /// Move a worktree to a new location.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    pub async fn worktree_move(
        &self,
        old_path: impl AsRef<Path>,
        new_path: impl AsRef<Path>,
    ) -> Result<(), GitError> {
        let old = old_path.as_ref().to_string_lossy();
        let new = new_path.as_ref().to_string_lossy();
        self.cmd.run(&["worktree", "move", &old, &new]).await?;
        Ok(())
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
        self.invalidate_cache().await;
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
        self.invalidate_cache().await;
        Ok(())
    }

    /// Rename a branch.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn branch_rename(&self, old: &str, new: &str, force: bool) -> Result<(), GitError> {
        if force {
            self.cmd.run(&["branch", "-M", old, new]).await?;
        } else {
            self.cmd.run(&["branch", "-m", old, new]).await?;
        }
        self.invalidate_cache().await;
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
        self.invalidate_cache().await;
        Ok(())
    }

    /// Switch to a branch (modern alternative to checkout).
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn switch(&self, branch: &str, create: bool) -> Result<(), GitError> {
        let mut args = vec!["switch"];
        if create {
            args.push("-c");
        }
        args.push(branch);
        let out = self.cmd.run(&args).await;
        if let Err(GitError::CommandFailed { ref stderr, .. }) = out {
            if stderr.contains("did not match") || stderr.contains("not found") {
                return Err(GitError::BranchNotFound(branch.to_string()));
            }
        }
        out?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Restore working tree files (modern alternative to checkout --).
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    pub async fn restore(
        &self,
        paths: &[impl AsRef<Path>],
        staged: bool,
        source: Option<&str>,
    ) -> Result<(), GitError> {
        let mut args = vec!["restore"];
        if staged {
            args.push("--staged");
        }
        if let Some(src) = source {
            args.push("--source");
            args.push(src);
        }
        for p in paths {
            if let Some(s) = p.as_ref().to_str() {
                args.push(s);
            }
        }
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
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

    /// Commit with options.
    pub async fn commit_opts(&self, opts: &CommitOptions<'_>) -> Result<String, GitError> {
        let mut args: Vec<String> = vec!["commit".into(), "-m".into(), opts.message.into()];
        if opts.no_verify {
            args.push("--no-verify".into());
        }
        if opts.amend {
            args.push("--amend".into());
        }
        if opts.signoff {
            args.push("--signoff".into());
        }
        if opts.paths.is_empty() {
            args.push("-a".into());
        } else {
            args.push("--".into());
            for p in opts.paths {
                args.push(p.to_string_lossy().into());
            }
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let _out = self.cmd.run(&args_ref).await?;
        self.invalidate_cache().await;
        let sha = self.head_commit().await?;
        #[cfg(feature = "tracing")]
        debug!(%sha, "committed");
        Ok(sha.to_string())
    }

    /// Commit with `message`. If `paths` is empty, commits all changes (`-a`).
    pub async fn commit(
        &self,
        message: &str,
        paths: &[impl AsRef<Path>],
        no_verify: bool,
    ) -> Result<String, GitError> {
        let paths: Vec<&Path> = paths.iter().map(|p| p.as_ref()).collect();
        self.commit_opts(&CommitOptions {
            message,
            paths: &paths,
            no_verify,
            amend: false,
            signoff: false,
        })
        .await
    }

    /// Push with options.
    pub async fn push_opts(&self, opts: &PushOptions<'_>) -> Result<(), GitError> {
        let mut args = vec!["push", opts.remote, opts.branch];
        if opts.force {
            args.push("--force");
        }
        if opts.force_with_lease {
            args.push("--force-with-lease");
        }
        if opts.set_upstream {
            args.push("--set-upstream");
        }
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Push `branch` to `remote`.
    pub async fn push(&self, remote: &str, branch: &str, force: bool) -> Result<(), GitError> {
        self.push_opts(&PushOptions {
            remote,
            branch,
            force_with_lease: force,
            ..Default::default()
        })
        .await
    }

    /// Push with `--force` (not `--force-with-lease`).
    pub async fn push_force(&self, remote: &str, branch: &str) -> Result<(), GitError> {
        self.cmd.run(&["push", "--force", remote, branch]).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Fetch with options.
    pub async fn fetch_opts(&self, opts: &FetchOptions<'_>) -> Result<(), GitError> {
        let mut args: Vec<String> = vec!["fetch".into(), opts.remote.into()];
        if opts.prune {
            args.push("--prune".into());
        }
        if opts.tags {
            args.push("--tags".into());
        }
        if let Some(depth) = opts.depth {
            args.push("--depth".into());
            args.push(depth.to_string());
        }
        if let Some(filter) = opts.filter {
            args.push("--filter".into());
            args.push(filter.into());
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd.run(&args_ref).await?;
        Ok(())
    }

    /// Fetch from `remote`.
    pub async fn fetch(&self, remote: &str) -> Result<(), GitError> {
        self.fetch_opts(&FetchOptions {
            remote,
            ..Default::default()
        })
        .await
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

    /// Add a new remote.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn remote_add(&self, name: &str, url: &str) -> Result<(), GitError> {
        self.cmd.run(&["remote", "add", name, url]).await?;
        Ok(())
    }

    /// Remove a remote.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn remote_remove(&self, name: &str) -> Result<(), GitError> {
        self.cmd.run(&["remote", "remove", name]).await?;
        Ok(())
    }

    /// Rename a remote.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn remote_rename(&self, old: &str, new: &str) -> Result<(), GitError> {
        self.cmd.run(&["remote", "rename", old, new]).await?;
        Ok(())
    }

    /// List refs on a remote without fetching.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn ls_remote(
        &self,
        remote: &str,
        refs: Option<&[&str]>,
    ) -> Result<Vec<(String, String)>, GitError> {
        let mut args = vec!["ls-remote", remote];
        if let Some(r) = refs {
            for ref_name in r {
                args.push(ref_name);
            }
        }
        let out = self.cmd.run(&args).await?;
        let mut result = Vec::new();
        for line in out.stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                result.push((parts[0].to_string(), parts[1].to_string()));
            }
        }
        Ok(result)
    }

    /// Pull from a remote branch.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn pull(&self, remote: &str, branch: &str, rebase: bool) -> Result<(), GitError> {
        let mut args = vec!["pull", remote, branch];
        if rebase {
            args.push("--rebase");
        } else {
            args.push("--no-rebase");
        }
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Get unstaged diff.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn diff(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["diff"]).await?;
        Ok(out.stdout)
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

    /// Get a structured diff of unstaged changes.
    pub async fn diff_structured(&self) -> Result<Vec<crate::types::FileDiff>, GitError> {
        let out = self.cmd.run(&["diff"]).await?;
        crate::parse::parse_diff(&out.stdout)
    }

    /// Get a structured diff of staged changes.
    pub async fn diff_cached_structured(&self) -> Result<Vec<crate::types::FileDiff>, GitError> {
        let out = self.cmd.run(&["diff", "--cached"]).await?;
        crate::parse::parse_diff(&out.stdout)
    }

    /// Get a structured diff for specific paths.
    pub async fn diff_files_structured(
        &self,
        paths: &[impl AsRef<Path>],
    ) -> Result<Vec<crate::types::FileDiff>, GitError> {
        let mut args: Vec<String> = vec!["diff".into(), "--".into()];
        for p in paths {
            args.push(p.as_ref().to_string_lossy().into_owned());
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let out = self.cmd.run(&args_ref).await?;
        crate::parse::parse_diff(&out.stdout)
    }

    /// Stage all changes (including untracked).
    pub async fn add_all(&self) -> Result<(), GitError> {
        self.cmd.run(&["add", "-A"]).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Stage a specific path.
    pub async fn add(&self, path: impl AsRef<Path>) -> Result<(), GitError> {
        let path_str = path.as_ref().to_string_lossy();
        self.cmd.run(&["add", &path_str]).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Move or rename a tracked file.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    pub async fn mv(
        &self,
        source: impl AsRef<Path>,
        dest: impl AsRef<Path>,
    ) -> Result<(), GitError> {
        let src = source.as_ref().to_string_lossy();
        let dst = dest.as_ref().to_string_lossy();
        self.cmd.run(&["mv", &src, &dst]).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Remove tracked files.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all))]
    pub async fn rm(&self, paths: &[impl AsRef<Path>], cached: bool) -> Result<(), GitError> {
        let mut args: Vec<String> = vec!["rm".into()];
        if cached {
            args.push("--cached".into());
        }
        for p in paths {
            args.push(p.as_ref().to_string_lossy().into_owned());
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd.run(&args_ref).await?;
        self.invalidate_cache().await;
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
        self.invalidate_cache().await;
        Ok(())
    }

    /// Pop the latest stash.
    pub async fn stash_pop(&self) -> Result<(), GitError> {
        self.cmd.run(&["stash", "pop"]).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Drop a stash entry.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn stash_drop(&self, index: Option<usize>) -> Result<(), GitError> {
        let mut args: Vec<String> = vec!["stash".into(), "drop".into()];
        if let Some(i) = index {
            args.push(format!("stash@{{{i}}}"));
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd.run(&args_ref).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Apply a stash entry without removing it.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn stash_apply(&self, index: Option<usize>) -> Result<(), GitError> {
        let mut args: Vec<String> = vec!["stash".into(), "apply".into()];
        if let Some(i) = index {
            args.push(format!("stash@{{{i}}}"));
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd.run(&args_ref).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Show the diff of a stash entry.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn stash_show(&self, index: Option<usize>) -> Result<String, GitError> {
        let mut args: Vec<String> = vec!["stash".into(), "show".into(), "-p".into()];
        if let Some(i) = index {
            args.push(format!("stash@{{{i}}}"));
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let out = self.cmd.run(&args_ref).await?;
        Ok(out.stdout)
    }

    /// Merge with options.
    pub async fn merge_opts(&self, opts: &MergeOptions<'_>) -> Result<(), GitError> {
        let mut args = vec!["merge", opts.branch];
        if opts.no_edit {
            args.push("--no-edit");
        }
        if opts.no_ff {
            args.push("--no-ff");
        }
        if opts.squash {
            args.push("--squash");
        }
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Merge branch into current HEAD (mutating).
    pub async fn merge(&self, branch: &str, no_edit: bool) -> Result<(), GitError> {
        self.merge_opts(&MergeOptions {
            branch,
            no_edit,
            ..Default::default()
        })
        .await
    }

    /// Find the best common ancestor(s) between commits.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn merge_base(&self, commits: &[&str]) -> Result<String, GitError> {
        let mut args = vec!["merge-base"];
        for c in commits {
            args.push(c);
        }
        let out = self.cmd.run(&args).await?;
        Ok(out.stdout.trim().to_string())
    }

    /// Rebase with options.
    pub async fn rebase_opts(&self, opts: &RebaseOptions<'_>) -> Result<(), GitError> {
        let mut args = vec!["rebase"];
        if opts.interactive {
            args.push("--interactive");
        }
        if opts.autosquash {
            args.push("--autosquash");
        }
        if let Some(onto) = opts.onto {
            args.push("--onto");
            args.push(onto);
        }
        args.push(opts.branch);
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Rebase current HEAD onto branch.
    pub async fn rebase(&self, branch: &str) -> Result<(), GitError> {
        self.rebase_opts(&RebaseOptions {
            branch,
            ..Default::default()
        })
        .await
    }

    /// Abort an in-progress rebase.
    pub async fn rebase_abort(&self) -> Result<(), GitError> {
        self.cmd.run(&["rebase", "--abort"]).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Continue an in-progress rebase after conflicts are resolved.
    pub async fn rebase_continue(&self) -> Result<(), GitError> {
        self.cmd
            .run_with_env(&["rebase", "--continue"], &[("GIT_EDITOR", "true")])
            .await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Get the commit log.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
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

    /// Stream commit log entries asynchronously.
    ///
    /// Returns a [`Stream`](tokio_stream::Stream) that yields
    /// [`GitLogEntry`](crate::types::GitLogEntry) values as they are produced
    /// by `git log`.  This avoids buffering the entire log in memory.
    ///
    /// Available only when the **`stream`** feature is enabled.
    #[cfg(feature = "stream")]
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn log_stream(
        &self,
    ) -> Result<impl Stream<Item = Result<crate::types::GitLogEntry, GitError>>, GitError> {
        let mut child = tokio::process::Command::new(self.cmd.git_bin())
            .current_dir(self.cmd.cwd())
            .args(["log", "--format=%H|%s|%an|%at"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("LC_ALL", "C")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn git log: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| GitError::Io("missing stdout".to_string()))?;
        let lines = LinesStream::new(BufReader::new(stdout).lines());
        Ok(LogStream {
            _child: child,
            lines: Box::pin(lines),
        })
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
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn config_get(&self, key: &str) -> Result<Option<String>, GitError> {
        let out = self.cmd.run(&["config", key]).await;
        match out {
            Ok(o) => Ok(Some(o.stdout.trim().to_string())),
            Err(GitError::CommandFailed {
                ref stderr,
                ref stdout,
                exit_code,
                ..
            }) if stderr.contains("not in config")
                || stderr.contains("has no value")
                || (exit_code == 1 && stderr.is_empty() && stdout.is_empty()) =>
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

    /// Unset a git config value.
    ///
    /// Returns `Ok(())` even if the key did not exist.
    pub async fn config_unset(&self, key: &str) -> Result<(), GitError> {
        let out = self.cmd.run(&["config", "--unset", key]).await;
        match out {
            Ok(_) => Ok(()),
            Err(GitError::CommandFailed {
                ref stderr,
                exit_code,
                ..
            }) if exit_code == 5 || stderr.contains("not in config") => Ok(()),
            Err(other) => Err(other),
        }
    }

    /// List all tags.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
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
        self.invalidate_cache().await;
        Ok(())
    }

    /// Delete a tag.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn tag_delete(&self, name: &str) -> Result<(), GitError> {
        self.cmd.run(&["tag", "-d", name]).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Create a signed (annotated) tag.
    ///
    /// If `gpg_key` is `Some`, it is passed as `-u <key>`; otherwise the
    /// default signing key configured in git is used.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn tag_create_signed(
        &self,
        name: &str,
        message: &str,
        gpg_key: Option<&str>,
    ) -> Result<(), GitError> {
        let mut args: Vec<String> = vec!["tag".into(), "-s".into(), "-m".into(), message.into()];
        if let Some(key) = gpg_key {
            args.push("-u".into());
            args.push(key.into());
        }
        args.push(name.into());
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd.run(&args_ref).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Verify the GPG/SSH signature of a tag.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn verify_tag(&self, name: &str) -> Result<crate::types::GitVerification, GitError> {
        let out = self.cmd.run(&["tag", "-v", name]).await;
        match out {
            Ok(_) => Ok(crate::types::GitVerification {
                valid: true,
                signer: None,
                fingerprint: None,
                status: "G".to_string(),
            }),
            Err(GitError::CommandFailed {
                ref stderr,
                exit_code,
                ..
            }) if exit_code == 1
                && (stderr.contains("no signature")
                    || stderr.contains("Can't check signature")
                    || stderr.contains("no GPG signature")) =>
            {
                Ok(crate::types::GitVerification {
                    valid: false,
                    signer: None,
                    fingerprint: None,
                    status: "N".to_string(),
                })
            }
            Err(other) => Err(other),
        }
    }

    /// Show file contents at a given revision.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn show(&self, path: &str, rev: Option<&str>) -> Result<String, GitError> {
        let spec = match rev {
            Some(r) => format!("{r}:{path}"),
            None => format!("HEAD:{path}"),
        };
        let out = self.cmd.run(&["show", &spec]).await?;
        Ok(out.stdout)
    }

    /// Resolve a revision to a full SHA.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn rev_parse(&self, rev: &str) -> Result<String, GitError> {
        let out = self.cmd.run(&["rev-parse", rev]).await?;
        Ok(out.stdout.trim().to_string())
    }

    /// Get blame information for a file.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn blame(&self, path: &str) -> Result<String, GitError> {
        let out = self.cmd.run(&["blame", "--line-porcelain", path]).await?;
        Ok(out.stdout)
    }

    /// Get structured blame information for a file.
    pub async fn blame_structured(
        &self,
        path: &str,
    ) -> Result<Vec<crate::types::BlameLine>, GitError> {
        let out = self.cmd.run(&["blame", "--line-porcelain", path]).await?;
        crate::parse::parse_blame(&out.stdout)
    }

    /// Stream blame information for a file line-by-line.
    #[cfg(feature = "stream")]
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn blame_stream(
        &self,
        path: &str,
    ) -> Result<impl Stream<Item = Result<crate::types::BlameLine, GitError>>, GitError> {
        let mut child = tokio::process::Command::new(self.cmd.git_bin())
            .current_dir(self.cmd.cwd())
            .args(["blame", "--line-porcelain", path])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("LC_ALL", "C")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn git blame: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| GitError::Io("missing stdout".to_string()))?;
        let lines = LinesStream::new(BufReader::new(stdout).lines());
        Ok(BlameStream {
            _child: child,
            lines: Box::pin(lines),
            buf_sha: String::new(),
            buf_author: String::new(),
            buf_author_mail: String::new(),
            buf_author_time: String::new(),
        })
    }

    /// Generate patches for a commit range using `git format-patch --stdout`.
    pub async fn format_patch(&self, range: &str) -> Result<Vec<crate::types::Patch>, GitError> {
        let out = self.cmd.run(&["format-patch", "--stdout", range]).await?;
        crate::parse::parse_format_patch(&out.stdout)
    }

    /// Apply a patch string to the working tree.
    ///
    /// When `dry_run` is `true`, only checks whether the patch can be applied
    /// without making any changes.
    pub async fn apply_patch(&self, patch: &str, dry_run: bool) -> Result<(), GitError> {
        let tmp = std::env::temp_dir().join(format!("gitr-patch-{}", std::process::id()));
        tokio::fs::write(&tmp, patch)
            .await
            .map_err(|e| GitError::Io(e.to_string()))?;
        let result = self.apply_patch_file(&tmp, dry_run).await;
        // Best-effort cleanup of the temporary patch file.
        #[allow(unused_must_use)]
        let _ = tokio::fs::remove_file(&tmp).await;
        result
    }

    /// Apply a patch file to the working tree.
    ///
    /// When `dry_run` is `true`, only checks whether the patch can be applied
    /// without making any changes.
    pub async fn apply_patch_file(
        &self,
        path: impl AsRef<Path>,
        dry_run: bool,
    ) -> Result<(), GitError> {
        let path_str = path.as_ref().to_string_lossy();
        if dry_run {
            self.cmd.run(&["apply", "--check", &path_str]).await?;
        } else {
            self.cmd.run(&["apply", &path_str]).await?;
        }
        Ok(())
    }

    /// List reflog entries for a ref (defaults to HEAD).
    pub async fn reflog_list(
        &self,
        ref_name: Option<&str>,
    ) -> Result<Vec<crate::types::ReflogEntry>, GitError> {
        let ref_name = ref_name.unwrap_or("HEAD");
        let out = self
            .cmd
            .run(&[
                "reflog",
                "show",
                "--format=%H|%an|%ae|%at|%gs|%gd",
                ref_name,
            ])
            .await?;
        crate::parse::parse_reflog(&out.stdout)
    }

    /// Expire reflog entries older than `expire_time` for a ref.
    ///
    /// `expire_time` can be an absolute timestamp or a relative date
    /// (e.g. "2.weeks.ago"). If `None`, uses git's default expiration.
    pub async fn reflog_expire(
        &self,
        ref_name: &str,
        expire_time: Option<&str>,
    ) -> Result<(), GitError> {
        let mut args = vec!["reflog", "expire", ref_name];
        if let Some(time) = expire_time {
            args.push("--expire");
            args.push(time);
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// List installed hooks in `.git/hooks/`.
    pub async fn hooks_list(&self) -> Result<Vec<crate::types::Hook>, GitError> {
        let hooks_dir = self.root.join(".git").join("hooks");
        let mut hooks = Vec::new();
        let mut entries = tokio::fs::read_dir(&hooks_dir)
            .await
            .map_err(|e| GitError::Io(format!("failed to read hooks dir: {e}")))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| GitError::Io(format!("failed to read hooks dir entry: {e}")))?
        {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.starts_with('.') {
                continue;
            }
            let active = is_executable(&path).await;
            hooks.push(crate::types::Hook { name, path, active });
        }
        Ok(hooks)
    }

    /// Install a hook script.
    ///
    /// The script is written to `.git/hooks/<name>` and made executable on Unix.
    pub async fn hook_install(&self, name: &str, script: &str) -> Result<(), GitError> {
        let hook_path = self.root.join(".git").join("hooks").join(name);
        tokio::fs::write(&hook_path, script)
            .await
            .map_err(|e| GitError::Io(format!("failed to write hook: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&hook_path)
                .await
                .map_err(|e| GitError::Io(format!("failed to read hook metadata: {e}")))?
                .permissions();
            perms.set_mode(perms.mode() | 0o111);
            tokio::fs::set_permissions(&hook_path, perms)
                .await
                .map_err(|e| GitError::Io(format!("failed to set hook permissions: {e}")))?;
        }
        Ok(())
    }

    /// Remove a hook.
    pub async fn hook_remove(&self, name: &str) -> Result<(), GitError> {
        let hook_path = self.root.join(".git").join("hooks").join(name);
        tokio::fs::remove_file(&hook_path)
            .await
            .map_err(|e| GitError::Io(format!("failed to remove hook: {e}")))?;
        Ok(())
    }

    /// Run a hook and capture its output.
    ///
    /// Uses the default 60-second timeout.
    pub async fn run_hook(&self, name: &str) -> Result<crate::types::HookOutput, GitError> {
        self.run_hook_with_timeout(name, Duration::from_secs(60))
            .await
    }

    /// Run a hook with a custom timeout and capture its output.
    pub async fn run_hook_with_timeout(
        &self,
        name: &str,
        timeout: Duration,
    ) -> Result<crate::types::HookOutput, GitError> {
        let hook_path = self.root.join(".git").join("hooks").join(name);
        let out = tokio::time::timeout(
            timeout,
            tokio::process::Command::new(&hook_path)
                .current_dir(&self.root)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_ASKPASS", "echo")
                .env("LC_ALL", "C")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| GitError::Timeout(timeout, format!("hook {name}")))?
        .map_err(|e| GitError::Io(format!("failed to run hook: {e}")))?;
        Ok(crate::types::HookOutput {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            exit_code: out.status.code().unwrap_or(-1),
        })
    }

    /// Run a hook and stream its stdout/stderr line-by-line.
    ///
    /// Each line is sent through the corresponding channel.  The method
    /// returns the process exit code when the hook finishes.
    ///
    /// A default timeout of 60 seconds is applied.
    pub async fn run_hook_streaming(
        &self,
        name: &str,
        stdout_tx: tokio::sync::mpsc::Sender<String>,
        stderr_tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<i32, GitError> {
        let hook_path = self.root.join(".git").join("hooks").join(name);
        let mut child = tokio::process::Command::new(&hook_path)
            .current_dir(&self.root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("LC_ALL", "C")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn hook: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| GitError::Io("missing stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| GitError::Io("missing stderr".to_string()))?;

        let stdout_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Some(line) = reader.next_line().await.transpose() {
                let line = match line {
                    Ok(l) => l,
                    Err(e) => return Err(GitError::Io(format!("stdout read error: {e}"))),
                };
                if stdout_tx.send(line).await.is_err() {
                    break;
                }
            }
            Ok(())
        });

        let stderr_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Some(line) = reader.next_line().await.transpose() {
                let line = match line {
                    Ok(l) => l,
                    Err(e) => return Err(GitError::Io(format!("stderr read error: {e}"))),
                };
                if stderr_tx.send(line).await.is_err() {
                    break;
                }
            }
            Ok(())
        });

        let timeout_dur = Duration::from_secs(60);
        let status = match tokio::time::timeout(timeout_dur, child.wait()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(GitError::Io(format!("failed to wait for hook: {e}"))),
            Err(_) => {
                // Best-effort kill of the timed-out hook process.
                #[allow(unused_must_use)]
                let _ = child.kill().await;
                return Err(GitError::Timeout(timeout_dur, format!("hook {name}")));
            }
        };

        stdout_handle
            .await
            .map_err(|e| GitError::Io(format!("stdout task panicked: {e}")))??;
        stderr_handle
            .await
            .map_err(|e| GitError::Io(format!("stderr task panicked: {e}")))??;

        Ok(status.code().unwrap_or(-1))
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
        self.invalidate_cache().await;
        Ok(())
    }

    /// Start a bisect session.
    pub async fn bisect_start(
        &self,
        bad: Option<&str>,
        good: &[&str],
    ) -> Result<crate::types::BisectState, GitError> {
        let mut args: Vec<String> = vec!["bisect".into(), "start".into()];
        if let Some(b) = bad {
            args.push(b.into());
        }
        for g in good {
            args.push((*g).into());
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd.run(&args_ref).await?;
        self.bisect_state().await
    }

    /// Mark a commit as bad during bisect.
    pub async fn bisect_bad(
        &self,
        commit: Option<&str>,
    ) -> Result<crate::types::BisectState, GitError> {
        let mut args = vec!["bisect", "bad"];
        if let Some(c) = commit {
            args.push(c);
        }
        self.cmd.run(&args).await?;
        self.bisect_state().await
    }

    /// Mark a commit as good during bisect.
    pub async fn bisect_good(
        &self,
        commit: Option<&str>,
    ) -> Result<crate::types::BisectState, GitError> {
        let mut args = vec!["bisect", "good"];
        if let Some(c) = commit {
            args.push(c);
        }
        self.cmd.run(&args).await?;
        self.bisect_state().await
    }

    /// Reset an active bisect session.
    pub async fn bisect_reset(&self) -> Result<(), GitError> {
        self.cmd.run(&["bisect", "reset"]).await?;
        Ok(())
    }

    /// Run a command automatically during bisect.
    pub async fn bisect_run(&self, command: &str) -> Result<String, GitError> {
        let out = self.cmd.run(&["bisect", "run", command]).await?;
        Ok(out.stdout)
    }

    async fn bisect_state(&self) -> Result<crate::types::BisectState, GitError> {
        let out = self.cmd.run(&["rev-parse", "BISECT_HEAD"]).await;
        let current = match out {
            Ok(o) => {
                let sha = o.stdout.trim();
                if sha.is_empty() {
                    None
                } else {
                    Some(sha.to_string())
                }
            }
            Err(_) => None,
        };
        Ok(crate::types::BisectState { current })
    }

    /// List git notes.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn notes_list(
        &self,
        namespace: Option<&str>,
    ) -> Result<Vec<crate::types::GitNote>, GitError> {
        let mut args = vec!["notes".to_string(), "list".to_string()];
        if let Some(ns) = namespace {
            args.push(format!("--ref={ns}"));
        }
        let out = self.cmd.run(&args).await?;
        let mut notes = Vec::new();
        for line in out.stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let commit = parts.next().map(String::from);
            let object = parts.next().map(String::from);
            if let (Some(commit), Some(object)) = (commit, object) {
                notes.push(crate::types::GitNote { object, commit });
            }
        }
        Ok(notes)
    }

    /// Show the note for a given object.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn notes_show(
        &self,
        object: &str,
        namespace: Option<&str>,
    ) -> Result<String, GitError> {
        let mut args = vec!["notes".to_string(), "show".to_string()];
        if let Some(ns) = namespace {
            args.push(format!("--ref={ns}"));
        }
        args.push(object.to_string());
        let out = self.cmd.run(&args).await?;
        Ok(out.stdout)
    }

    /// Add a note to an object.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn notes_add(
        &self,
        message: &str,
        object: &str,
        namespace: Option<&str>,
        force: bool,
    ) -> Result<(), GitError> {
        let mut args = vec!["notes".to_string(), "add".to_string()];
        if let Some(ns) = namespace {
            args.push(format!("--ref={ns}"));
        }
        if force {
            args.push("--force".to_string());
        }
        args.push("-m".to_string());
        args.push(message.to_string());
        args.push(object.to_string());
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// Remove the note from an object.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn notes_remove(
        &self,
        object: &str,
        namespace: Option<&str>,
    ) -> Result<(), GitError> {
        let mut args = vec!["notes".to_string(), "remove".to_string()];
        if let Some(ns) = namespace {
            args.push(format!("--ref={ns}"));
        }
        args.push(object.to_string());
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// List stash entries.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
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

    /// Cherry-pick with options.
    pub async fn cherry_pick_opts(&self, opts: &CherryPickOptions<'_>) -> Result<(), GitError> {
        let mut args = vec!["cherry-pick"];
        if opts.no_commit {
            args.push("--no-commit");
        }
        for c in opts.commits {
            args.push(c);
        }
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Cherry-pick one or more commits.
    pub async fn cherry_pick(&self, commits: &[&str]) -> Result<(), GitError> {
        self.cherry_pick_opts(&CherryPickOptions {
            commits,
            ..Default::default()
        })
        .await
    }

    /// Revert one or more commits.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn revert(&self, commits: &[&str], no_edit: bool) -> Result<(), GitError> {
        let mut args = vec!["revert"];
        if no_edit {
            args.push("--no-edit");
        }
        for c in commits {
            args.push(c);
        }
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Create a GPG-signed commit.
    ///
    /// If `gpg_key` is `Some`, it is passed as `-S <key>`; otherwise the
    /// default signing key configured in git is used.
    /// When `no_verify` is `true`, `--no-verify` is passed to skip hooks.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, paths)))]
    pub async fn commit_signed(
        &self,
        paths: &[&Path],
        message: &str,
        gpg_key: Option<&str>,
        no_verify: bool,
    ) -> Result<(), GitError> {
        let mut args: Vec<String> = vec!["commit".into(), "-S".into()];
        if let Some(key) = gpg_key {
            args.push(key.into());
        }
        if no_verify {
            args.push("--no-verify".into());
        }
        for p in paths {
            args.push(p.to_string_lossy().to_string());
        }
        args.push("-m".into());
        args.push(message.into());
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.cmd.run(&args_ref).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Verify the GPG signature of a commit.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn verify_commit(
        &self,
        sha: &str,
    ) -> Result<crate::types::GitVerification, GitError> {
        let out = self
            .cmd
            .run(&["log", "-1", "--format=%G?|%GS|%GK", sha])
            .await?;
        let line = out.stdout.trim();
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() != 3 {
            return Err(GitError::Parse(format!("unexpected verify output: {line}")));
        }
        let status = parts[0];
        let valid = status == "G" || status == "U";
        let signer = if parts[1].is_empty() {
            None
        } else {
            Some(parts[1].to_string())
        };
        let fingerprint = if parts[2].is_empty() {
            None
        } else {
            Some(parts[2].to_string())
        };
        Ok(crate::types::GitVerification {
            valid,
            signer,
            fingerprint,
            status: status.to_string(),
        })
    }

    /// List submodules and their current state.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn submodule_list(&self) -> Result<Vec<crate::types::GitSubmodule>, GitError> {
        let out = self
            .cmd
            .run(&["submodule", "status", "--recursive"])
            .await?;
        parse::parse_submodules(&out.stdout)
    }

    /// Add a new submodule.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, path)))]
    pub async fn submodule_add(&self, url: &str, path: impl AsRef<Path>) -> Result<(), GitError> {
        self.cmd
            .run(&[
                "submodule",
                "add",
                url,
                path.as_ref().to_string_lossy().as_ref(),
            ])
            .await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Update submodules (optionally initializing and recursing).
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn submodule_update(&self, init: bool, recursive: bool) -> Result<(), GitError> {
        let mut args = vec!["submodule", "update"];
        if init {
            args.push("--init");
        }
        if recursive {
            args.push("--recursive");
        }
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Deinitialize a submodule.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, path)))]
    pub async fn submodule_deinit(
        &self,
        path: impl AsRef<Path>,
        force: bool,
    ) -> Result<(), GitError> {
        let mut args = vec!["submodule", "deinit"];
        if force {
            args.push("-f");
        }
        let path_str = path.as_ref().to_string_lossy();
        args.push(&path_str);
        self.cmd.run(&args).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// Synchronize submodule remote URLs.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn submodule_sync(&self) -> Result<(), GitError> {
        self.cmd.run(&["submodule", "sync", "--recursive"]).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    /// List tracked files matching optional filters.
    ///
    /// * `deleted` — include deleted files (`--deleted`).
    /// * `others` — include untracked files (`--others`).
    /// * `exclude_standard` — respect `.gitignore` when listing others
    ///   (`--exclude-standard`).
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn ls_files(
        &self,
        deleted: bool,
        others: bool,
        exclude_standard: bool,
    ) -> Result<Vec<String>, GitError> {
        let mut args = vec!["ls-files"];
        if deleted {
            args.push("--deleted");
        }
        if others {
            args.push("--others");
        }
        if exclude_standard {
            args.push("--exclude-standard");
        }
        let out = self.cmd.run(&args).await?;
        Ok(out.stdout.lines().map(|l| l.to_string()).collect())
    }

    /// Stream `git ls-files` results line-by-line.
    #[cfg(feature = "stream")]
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn ls_files_stream(
        &self,
        deleted: bool,
        others: bool,
        exclude_standard: bool,
    ) -> Result<impl Stream<Item = Result<String, GitError>>, GitError> {
        let mut args = vec!["ls-files"];
        if deleted {
            args.push("--deleted");
        }
        if others {
            args.push("--others");
        }
        if exclude_standard {
            args.push("--exclude-standard");
        }
        let mut child = tokio::process::Command::new(self.cmd.git_bin())
            .current_dir(self.cmd.cwd())
            .args(&args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("LC_ALL", "C")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn git ls-files: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| GitError::Io("missing stdout".to_string()))?;
        let lines = LinesStream::new(BufReader::new(stdout).lines());
        Ok(LsFilesStream {
            _child: child,
            lines: Box::pin(lines),
        })
    }

    /// Check which paths are ignored by git.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, paths)))]
    pub async fn check_ignore(&self, paths: &[impl AsRef<Path>]) -> Result<Vec<String>, GitError> {
        let mut args = vec!["check-ignore".to_string()];
        for p in paths {
            args.push(p.as_ref().to_string_lossy().to_string());
        }
        let out = self.cmd.run(&args).await?;
        Ok(out.stdout.lines().map(|l| l.to_string()).collect())
    }

    /// Check git attributes for given paths.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, paths, attrs)))]
    pub async fn check_attr(
        &self,
        paths: &[impl AsRef<Path>],
        attrs: &[&str],
    ) -> Result<Vec<crate::types::GitAttr>, GitError> {
        let mut args: Vec<String> = vec!["check-attr".to_string(), "-z".to_string()];
        for a in attrs {
            args.push(a.to_string());
        }
        args.push("--".to_string());
        for p in paths {
            args.push(p.as_ref().to_string_lossy().to_string());
        }
        let out = self.cmd.run(&args).await?;
        let mut results = Vec::new();
        let parts: Vec<&str> = out.stdout.split('\0').collect();
        for chunk in parts.chunks(3) {
            if chunk.len() == 3 {
                results.push(crate::types::GitAttr {
                    path: chunk[0].to_string(),
                    attr: chunk[1].to_string(),
                    value: chunk[2].to_string(),
                });
            }
        }
        Ok(results)
    }

    /// Get the staged (cached) diff.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn diff_cached(&self) -> Result<String, GitError> {
        let out = self.cmd.run(&["diff", "--cached"]).await?;
        Ok(out.stdout)
    }

    /// Create an archive of `ref_name` (e.g. a tag or branch) at `output`.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, output)))]
    pub async fn archive(&self, ref_name: &str, output: impl AsRef<Path>) -> Result<(), GitError> {
        let output = output.as_ref().to_string_lossy();
        self.cmd.run(&["archive", ref_name, "-o", &output]).await?;
        Ok(())
    }

    /// Create a bundle file containing the specified refs.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, output)))]
    pub async fn bundle_create(
        &self,
        output: impl AsRef<Path>,
        refs: Option<&[&str]>,
    ) -> Result<(), GitError> {
        let output = output.as_ref().to_string_lossy();
        let mut args = vec![
            "bundle".to_string(),
            "create".to_string(),
            output.to_string(),
        ];
        if let Some(refs) = refs {
            for r in refs {
                args.push(r.to_string());
            }
        } else {
            args.push("--all".to_string());
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// List refs contained in a bundle file.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, path)))]
    pub async fn bundle_list_heads(&self, path: impl AsRef<Path>) -> Result<Vec<String>, GitError> {
        let path = path.as_ref().to_string_lossy();
        let out = self.cmd.run(&["bundle", "list-heads", &path]).await?;
        Ok(out.stdout.lines().map(|l| l.to_string()).collect())
    }

    /// Verify a bundle file is valid and can be applied.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, path)))]
    pub async fn bundle_verify(&self, path: impl AsRef<Path>) -> Result<(), GitError> {
        let path = path.as_ref().to_string_lossy();
        self.cmd.run(&["bundle", "verify", &path]).await?;
        Ok(())
    }

    /// Unbundle (fetch from) a bundle file into the repository.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, path)))]
    pub async fn bundle_unbundle(&self, path: impl AsRef<Path>) -> Result<Vec<String>, GitError> {
        let path = path.as_ref().to_string_lossy();
        let out = self.cmd.run(&["bundle", "unbundle", &path]).await?;
        Ok(out.stdout.lines().map(|l| l.to_string()).collect())
    }

    /// Compute the SHA of a blob from raw bytes without writing to the object DB.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, data)))]
    pub async fn hash_object(&self, data: &[u8]) -> Result<Oid, GitError> {
        let mut child = tokio::process::Command::new(self.cmd.git_bin())
            .current_dir(self.cmd.cwd())
            .args(["hash-object", "--stdin"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn git hash-object: {e}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| GitError::Io("missing stdin".to_string()))?;
        stdin
            .write_all(data)
            .await
            .map_err(|e| GitError::Io(format!("stdin write: {e}")))?;
        drop(stdin);

        let out = child
            .wait_with_output()
            .await
            .map_err(|e| GitError::Io(format!("wait: {e}")))?;
        if !out.status.success() {
            return Err(GitError::CommandFailed {
                command: "hash-object --stdin".to_string(),
                exit_code: out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                stdout: String::new(),
            });
        }
        Oid::new(String::from_utf8_lossy(&out.stdout).trim())
    }

    /// Write a blob to the object DB and return its OID.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, data)))]
    pub async fn write_blob(&self, data: &[u8]) -> Result<Oid, GitError> {
        let mut child = tokio::process::Command::new(self.cmd.git_bin())
            .current_dir(self.cmd.cwd())
            .args(["hash-object", "-w", "--stdin"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn git hash-object: {e}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| GitError::Io("missing stdin".to_string()))?;
        stdin
            .write_all(data)
            .await
            .map_err(|e| GitError::Io(format!("stdin write: {e}")))?;
        drop(stdin);

        let out = child
            .wait_with_output()
            .await
            .map_err(|e| GitError::Io(format!("wait: {e}")))?;
        if !out.status.success() {
            return Err(GitError::CommandFailed {
                command: "hash-object -w --stdin".to_string(),
                exit_code: out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                stdout: String::new(),
            });
        }
        Oid::new(String::from_utf8_lossy(&out.stdout).trim())
    }

    /// Create a tree object from entries and return its OID.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, entries)))]
    pub async fn mktree(&self, entries: &[TreeEntry]) -> Result<Oid, GitError> {
        // Build a tree object manually and write it via hash-object.
        // Tree format: sorted entries, each: "<mode> <path>\0<20-byte binary sha>"
        let mut sorted: Vec<&TreeEntry> = entries.iter().collect();
        sorted.sort_by_key(|e| &e.path);

        let mut input: Vec<u8> = Vec::new();
        for e in sorted {
            input.extend_from_slice(e.mode.as_bytes());
            input.push(b' ');
            input.extend_from_slice(e.path.as_bytes());
            input.push(0);
            let oid_bytes = hex_to_bytes(e.oid.as_ref())
                .map_err(|e| GitError::Parse(format!("invalid oid: {e}")))?;
            input.extend_from_slice(&oid_bytes);
        }

        let mut child = tokio::process::Command::new(self.cmd.git_bin())
            .current_dir(self.cmd.cwd())
            .args(["hash-object", "-t", "tree", "-w", "--stdin"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn git hash-object: {e}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| GitError::Io("missing stdin".to_string()))?;
        stdin
            .write_all(&input)
            .await
            .map_err(|e| GitError::Io(format!("stdin write: {e}")))?;
        drop(stdin);

        let out = child
            .wait_with_output()
            .await
            .map_err(|e| GitError::Io(format!("wait: {e}")))?;
        if !out.status.success() {
            return Err(GitError::CommandFailed {
                command: "hash-object -t tree -w --stdin".to_string(),
                exit_code: out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                stdout: String::new(),
            });
        }
        Oid::new(String::from_utf8_lossy(&out.stdout).trim())
    }

    /// Write a tree object to the object DB and return its OID.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, entries)))]
    pub async fn write_tree(&self, entries: &[TreeEntry]) -> Result<Oid, GitError> {
        self.mktree(entries).await
    }

    /// Read a git object by OID.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn read_object(&self, oid: &Oid) -> Result<ObjectContent, GitError> {
        let oid_str = oid.as_ref();
        let out = self.cmd.run(&["cat-file", "-p", oid_str]).await?;
        let kind_out = self.cmd.run(&["cat-file", "-t", oid_str]).await?;
        let kind_str = kind_out.stdout.trim();
        let kind = kind_str.parse().map_err(|e: GitError| {
            GitError::Parse(format!("invalid object kind for {oid}: {e}"))
        })?;
        Ok(ObjectContent {
            oid: oid.clone(),
            kind,
            size: out.stdout.len(),
            data: out.stdout.into_bytes(),
        })
    }

    /// Read a tree object and return its entries.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn read_tree(&self, oid: &Oid) -> Result<Vec<TreeEntry>, GitError> {
        let obj = self.read_object(oid).await?;
        let mut entries = Vec::new();
        let text = String::from_utf8_lossy(&obj.data);
        for line in text.lines() {
            // Format from `git cat-file -p <tree>`: "<mode> <type> <sha>\t<path>"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let mode = parts[0];
                let sha = parts[2];
                if let Some(tab_pos) = line.find('\t') {
                    let path = &line[tab_pos + 1..];
                    entries.push(TreeEntry {
                        mode: mode.to_string(),
                        path: path.to_string(),
                        oid: Oid::new(sha)?,
                    });
                }
            }
        }
        Ok(entries)
    }

    /// Read a blob object and return its raw bytes.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn read_blob(&self, oid: &Oid) -> Result<Vec<u8>, GitError> {
        let obj = self.read_object(oid).await?;
        Ok(obj.data)
    }

    /// Read a commit object and parse it.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn read_commit(&self, oid: &Oid) -> Result<crate::types::GitCommit, GitError> {
        let obj = self.read_object(oid).await?;
        let text = String::from_utf8_lossy(&obj.data);
        let mut tree = None;
        let mut parents = Vec::new();
        let mut author = String::new();
        let mut committer = String::new();
        let mut message = String::new();
        let mut in_message = false;
        for line in text.lines() {
            if in_message {
                message.push_str(line);
                message.push('\n');
            } else if line.is_empty() {
                in_message = true;
            } else if let Some(rest) = line.strip_prefix("tree ") {
                tree = Some(Oid::new(rest)?);
            } else if let Some(rest) = line.strip_prefix("parent ") {
                parents.push(Oid::new(rest)?);
            } else if let Some(rest) = line.strip_prefix("author ") {
                author = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("committer ") {
                committer = rest.to_string();
            }
        }
        let tree =
            tree.ok_or_else(|| GitError::Parse("missing tree in commit object".to_string()))?;
        Ok(crate::types::GitCommit {
            tree,
            parents,
            author,
            committer,
            message: message.trim_end_matches('\n').to_string(),
        })
    }

    /// Write a commit object and return its OID.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn write_commit(
        &self,
        parents: &[&Oid],
        tree: &Oid,
        message: &str,
    ) -> Result<Oid, GitError> {
        let mut content = format!("tree {}\n", tree.as_ref());
        for p in parents {
            content.push_str(&format!("parent {}\n", p.as_ref()));
        }
        content.push_str(&format!(
            "author gitr <gitr@localhost> {} +0000\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));
        content.push_str(&format!(
            "committer gitr <gitr@localhost> {} +0000\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));
        content.push('\n');
        content.push_str(message);
        content.push('\n');

        let mut child = tokio::process::Command::new(self.cmd.git_bin())
            .current_dir(self.cmd.cwd())
            .args(["hash-object", "-t", "commit", "-w", "--stdin"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn git hash-object: {e}")))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| GitError::Io("missing stdin".to_string()))?;
        stdin
            .write_all(content.as_bytes())
            .await
            .map_err(|e| GitError::Io(format!("stdin write: {e}")))?;
        drop(stdin);

        let out = child
            .wait_with_output()
            .await
            .map_err(|e| GitError::Io(format!("wait: {e}")))?;
        if !out.status.success() {
            return Err(GitError::CommandFailed {
                command: "hash-object -t commit -w --stdin".to_string(),
                exit_code: out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                stdout: String::new(),
            });
        }
        Oid::new(String::from_utf8_lossy(&out.stdout).trim())
    }

    /// Read the index.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn read_index(&self) -> Result<Vec<IndexEntry>, GitError> {
        let out = self.cmd.run(&["ls-files", "--stage"]).await?;
        let mut entries = Vec::new();
        for line in out.stdout.lines() {
            // Format: "<mode> <oid> <stage>\t<path>"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                if let Ok(mode) = parts[0].parse::<u32>() {
                    let path = parts[3..].join(" ");
                    entries.push(IndexEntry {
                        mode,
                        oid: Oid::new(parts[1])?,
                        path,
                    });
                }
            }
        }
        Ok(entries)
    }

    /// Update the index for a single path.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, path)))]
    pub async fn update_index(
        &self,
        path: impl AsRef<Path>,
        oid: &Oid,
        mode: u32,
    ) -> Result<(), GitError> {
        let path_str = path.as_ref().to_string_lossy();
        let oid_str = oid.as_ref();
        self.cmd
            .run(&[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("{:o},{oid_str},{path_str}", mode),
            ])
            .await?;
        Ok(())
    }

    /// Remove a path from the index.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self, path)))]
    pub async fn remove_index(&self, path: impl AsRef<Path>) -> Result<(), GitError> {
        let path_str = path.as_ref().to_string_lossy();
        self.cmd
            .run(&["update-index", "--remove", &path_str])
            .await?;
        Ok(())
    }

    /// Run `git grep` for `pattern` and return structured matches.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn grep(&self, pattern: &str) -> Result<Vec<crate::types::GitGrepResult>, GitError> {
        let out = self.cmd.run(&["grep", "-n", pattern]).await?;
        parse::parse_grep(&out.stdout)
    }

    /// Stream `git grep` results line-by-line.
    #[cfg(feature = "stream")]
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn grep_stream(
        &self,
        pattern: &str,
    ) -> Result<impl Stream<Item = Result<crate::types::GitGrepResult, GitError>>, GitError> {
        let mut child = tokio::process::Command::new(self.cmd.git_bin())
            .current_dir(self.cmd.cwd())
            .args(["grep", "-n", pattern])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "echo")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("LC_ALL", "C")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| GitError::Io(format!("failed to spawn git grep: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| GitError::Io("missing stdout".to_string()))?;
        let lines = LinesStream::new(BufReader::new(stdout).lines());
        Ok(GrepStream {
            _child: child,
            lines: Box::pin(lines),
        })
    }

    /// Describe the current commit in terms of the nearest tag.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn describe(&self, tags: bool, long: bool) -> Result<String, GitError> {
        let mut args = vec!["describe"];
        if tags {
            args.push("--tags");
        }
        if long {
            args.push("--long");
        }
        let out = self.cmd.run(&args).await?;
        Ok(out.stdout.trim().to_string())
    }

    /// Remove untracked files from the working tree.
    ///
    /// * `force` — actually remove files (`-f`). Without this `git clean` refuses
    ///   to run unless `clean.requireForce` is false.
    /// * `directories` — remove untracked directories in addition to files (`-d`).
    /// * `dry_run` — only show what would be removed (`-n`).
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn clean(
        &self,
        force: bool,
        directories: bool,
        dry_run: bool,
    ) -> Result<Vec<String>, GitError> {
        let mut args = vec!["clean"];
        if force {
            args.push("-f");
        }
        if directories {
            args.push("-d");
        }
        if dry_run {
            args.push("-n");
        }
        let out = self.cmd.run(&args).await?;
        Ok(out.stdout.lines().map(|l| l.to_string()).collect())
    }

    // ------------------------------------------------------------------
    // Git LFS
    // ------------------------------------------------------------------

    /// Start tracking file paths matching `patterns` with Git LFS.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn lfs_track(&self, patterns: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["lfs", "track"];
        for p in patterns {
            args.push(p);
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// Stop tracking file paths matching `patterns` with Git LFS.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn lfs_untrack(&self, patterns: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["lfs", "untrack"];
        for p in patterns {
            args.push(p);
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// List files tracked by Git LFS.
    ///
    /// Parses the output of `git lfs ls-files --long`.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn lfs_ls_files(&self) -> Result<Vec<GitLfsFile>, GitError> {
        let out = self.cmd.run(&["lfs", "ls-files", "--long"]).await?;
        let mut files = Vec::new();
        for line in out.stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }
            let oid = parts[1].to_string();
            let size = if parts[2] == "-" {
                None
            } else {
                parts[2].parse::<u64>().ok()
            };
            let path = if parts.len() > 3 {
                parts[3..].join(" ")
            } else {
                String::new()
            };
            files.push(GitLfsFile { oid, path, size });
        }
        Ok(files)
    }

    /// Lock a file path with Git LFS.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn lfs_lock(&self, path: &str) -> Result<(), GitError> {
        self.cmd.run(&["lfs", "lock", path]).await?;
        Ok(())
    }

    /// Unlock a file path with Git LFS.
    ///
    /// When `force` is `true`, passes `--force`.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn lfs_unlock(&self, path: &str, force: bool) -> Result<(), GitError> {
        let mut args = vec!["lfs", "unlock"];
        if force {
            args.push("--force");
        }
        args.push(path);
        self.cmd.run(&args).await?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Sparse checkout
    // ------------------------------------------------------------------

    /// Initialize sparse checkout.
    ///
    /// When `cone` is `true`, passes `--cone`.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn sparse_checkout_init(&self, cone: bool) -> Result<(), GitError> {
        let mut args = vec!["sparse-checkout", "init"];
        if cone {
            args.push("--cone");
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// Set the sparse-checkout paths, replacing the existing list.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn sparse_checkout_set(&self, paths: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["sparse-checkout", "set"];
        for p in paths {
            args.push(p);
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// Add paths to the sparse-checkout list.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn sparse_checkout_add(&self, paths: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["sparse-checkout", "add"];
        for p in paths {
            args.push(p);
        }
        self.cmd.run(&args).await?;
        Ok(())
    }

    /// Disable sparse checkout.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn sparse_checkout_disable(&self) -> Result<(), GitError> {
        self.cmd.run(&["sparse-checkout", "disable"]).await?;
        Ok(())
    }

    /// List the current sparse-checkout paths.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(self)))]
    pub async fn sparse_checkout_list(&self) -> Result<Vec<String>, GitError> {
        let out = self.cmd.run(&["sparse-checkout", "list"]).await?;
        Ok(out.stdout.lines().map(|l| l.to_string()).collect())
    }
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("odd length".to_string());
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks_exact(2) {
        let hi = (chunk[0] as char).to_digit(16).ok_or("invalid hex")?;
        let lo = (chunk[1] as char).to_digit(16).ok_or("invalid hex")?;
        bytes.push((hi * 16 + lo) as u8);
    }
    Ok(bytes)
}

async fn is_executable(path: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::metadata(path)
            .await
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
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

    async fn head_commit(&self) -> Result<Oid, GitError> {
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

    async fn commit_opts(&self, opts: &CommitOptions<'_>) -> Result<String, GitError> {
        self.commit_opts(opts).await
    }

    async fn push_opts(&self, opts: &PushOptions<'_>) -> Result<(), GitError> {
        self.push_opts(opts).await
    }

    async fn fetch_opts(&self, opts: &FetchOptions<'_>) -> Result<(), GitError> {
        self.fetch_opts(opts).await
    }

    async fn merge_tree(&self, base: &str, branch: &str) -> Result<GitMergeResult, GitError> {
        self.merge_tree(base, branch).await
    }

    async fn merge_opts(&self, opts: &MergeOptions<'_>) -> Result<(), GitError> {
        self.merge_opts(opts).await
    }

    async fn rebase_opts(&self, opts: &RebaseOptions<'_>) -> Result<(), GitError> {
        self.rebase_opts(opts).await
    }

    async fn stash(&self, message: Option<&str>) -> Result<(), GitError> {
        self.stash(message).await
    }

    async fn diff(&self) -> Result<String, GitError> {
        self.diff().await
    }

    async fn diff_structured(&self) -> Result<Vec<crate::types::FileDiff>, GitError> {
        self.diff_structured().await
    }

    async fn diff_cached_structured(&self) -> Result<Vec<crate::types::FileDiff>, GitError> {
        self.diff_cached_structured().await
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

    async fn config_unset(&self, key: &str) -> Result<(), GitError> {
        self.config_unset(key).await
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

    async fn blame_structured(&self, path: &str) -> Result<Vec<crate::types::BlameLine>, GitError> {
        self.blame_structured(path).await
    }

    async fn format_patch(&self, range: &str) -> Result<Vec<crate::types::Patch>, GitError> {
        self.format_patch(range).await
    }

    async fn apply_patch(&self, patch: &str, dry_run: bool) -> Result<(), GitError> {
        self.apply_patch(patch, dry_run).await
    }

    async fn apply_patch_file(&self, path: &Path, dry_run: bool) -> Result<(), GitError> {
        self.apply_patch_file(path, dry_run).await
    }

    async fn reflog_list(
        &self,
        ref_name: Option<&str>,
    ) -> Result<Vec<crate::types::ReflogEntry>, GitError> {
        self.reflog_list(ref_name).await
    }

    async fn reflog_expire(
        &self,
        ref_name: &str,
        expire_time: Option<&str>,
    ) -> Result<(), GitError> {
        self.reflog_expire(ref_name, expire_time).await
    }

    async fn hooks_list(&self) -> Result<Vec<crate::types::Hook>, GitError> {
        self.hooks_list().await
    }

    async fn hook_install(&self, name: &str, script: &str) -> Result<(), GitError> {
        self.hook_install(name, script).await
    }

    async fn hook_remove(&self, name: &str) -> Result<(), GitError> {
        self.hook_remove(name).await
    }

    async fn run_hook(&self, name: &str) -> Result<crate::types::HookOutput, GitError> {
        self.run_hook(name).await
    }

    async fn bisect_start(
        &self,
        bad: Option<&str>,
        good: &[&str],
    ) -> Result<crate::types::BisectState, GitError> {
        self.bisect_start(bad, good).await
    }

    async fn bisect_bad(
        &self,
        commit: Option<&str>,
    ) -> Result<crate::types::BisectState, GitError> {
        self.bisect_bad(commit).await
    }

    async fn bisect_good(
        &self,
        commit: Option<&str>,
    ) -> Result<crate::types::BisectState, GitError> {
        self.bisect_good(commit).await
    }

    async fn bisect_reset(&self) -> Result<(), GitError> {
        self.bisect_reset().await
    }

    async fn bisect_run(&self, command: &str) -> Result<String, GitError> {
        self.bisect_run(command).await
    }

    async fn notes_list(
        &self,
        namespace: Option<&str>,
    ) -> Result<Vec<crate::types::GitNote>, GitError> {
        self.notes_list(namespace).await
    }

    async fn notes_show(&self, object: &str, namespace: Option<&str>) -> Result<String, GitError> {
        self.notes_show(object, namespace).await
    }

    async fn notes_add(
        &self,
        message: &str,
        object: &str,
        namespace: Option<&str>,
        force: bool,
    ) -> Result<(), GitError> {
        self.notes_add(message, object, namespace, force).await
    }

    async fn notes_remove(&self, object: &str, namespace: Option<&str>) -> Result<(), GitError> {
        self.notes_remove(object, namespace).await
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

    async fn cherry_pick_opts(&self, opts: &CherryPickOptions<'_>) -> Result<(), GitError> {
        self.cherry_pick_opts(opts).await
    }

    async fn commit_signed(
        &self,
        paths: &[&Path],
        message: &str,
        gpg_key: Option<&str>,
        no_verify: bool,
    ) -> Result<(), GitError> {
        self.commit_signed(paths, message, gpg_key, no_verify).await
    }

    async fn verify_commit(&self, sha: &str) -> Result<crate::types::GitVerification, GitError> {
        self.verify_commit(sha).await
    }

    async fn submodule_list(&self) -> Result<Vec<crate::types::GitSubmodule>, GitError> {
        self.submodule_list().await
    }

    async fn submodule_add(&self, url: &str, path: &Path) -> Result<(), GitError> {
        self.submodule_add(url, path).await
    }

    async fn submodule_update(&self, init: bool, recursive: bool) -> Result<(), GitError> {
        self.submodule_update(init, recursive).await
    }

    async fn submodule_deinit(&self, path: &Path, force: bool) -> Result<(), GitError> {
        self.submodule_deinit(path, force).await
    }

    async fn submodule_sync(&self) -> Result<(), GitError> {
        self.submodule_sync().await
    }

    async fn ls_files(
        &self,
        deleted: bool,
        others: bool,
        exclude_standard: bool,
    ) -> Result<Vec<String>, GitError> {
        self.ls_files(deleted, others, exclude_standard).await
    }

    async fn diff_cached(&self) -> Result<String, GitError> {
        self.diff_cached().await
    }

    async fn archive(&self, ref_name: &str, output: &Path) -> Result<(), GitError> {
        self.archive(ref_name, output).await
    }

    async fn bundle_create(&self, output: &Path, refs: Option<&[&str]>) -> Result<(), GitError> {
        self.bundle_create(output, refs).await
    }

    async fn bundle_list_heads(&self, path: &Path) -> Result<Vec<String>, GitError> {
        self.bundle_list_heads(path).await
    }

    async fn bundle_verify(&self, path: &Path) -> Result<(), GitError> {
        self.bundle_verify(path).await
    }

    async fn bundle_unbundle(&self, path: &Path) -> Result<Vec<String>, GitError> {
        self.bundle_unbundle(path).await
    }

    async fn grep(&self, pattern: &str) -> Result<Vec<crate::types::GitGrepResult>, GitError> {
        self.grep(pattern).await
    }

    async fn check_ignore(&self, paths: &[&Path]) -> Result<Vec<String>, GitError> {
        self.check_ignore(paths).await
    }

    async fn check_attr(
        &self,
        paths: &[&Path],
        attrs: &[&str],
    ) -> Result<Vec<crate::types::GitAttr>, GitError> {
        self.check_attr(paths, attrs).await
    }

    async fn describe(&self, tags: bool, long: bool) -> Result<String, GitError> {
        self.describe(tags, long).await
    }

    async fn clean(
        &self,
        force: bool,
        directories: bool,
        dry_run: bool,
    ) -> Result<Vec<String>, GitError> {
        self.clean(force, directories, dry_run).await
    }

    async fn lfs_track(&self, patterns: &[&str]) -> Result<(), GitError> {
        self.lfs_track(patterns).await
    }

    async fn lfs_untrack(&self, patterns: &[&str]) -> Result<(), GitError> {
        self.lfs_untrack(patterns).await
    }

    async fn lfs_ls_files(&self) -> Result<Vec<crate::types::GitLfsFile>, GitError> {
        self.lfs_ls_files().await
    }

    async fn lfs_lock(&self, path: &str) -> Result<(), GitError> {
        self.lfs_lock(path).await
    }

    async fn lfs_unlock(&self, path: &str, force: bool) -> Result<(), GitError> {
        self.lfs_unlock(path, force).await
    }

    async fn sparse_checkout_init(&self, cone: bool) -> Result<(), GitError> {
        self.sparse_checkout_init(cone).await
    }

    async fn sparse_checkout_set(&self, paths: &[&str]) -> Result<(), GitError> {
        self.sparse_checkout_set(paths).await
    }

    async fn sparse_checkout_add(&self, paths: &[&str]) -> Result<(), GitError> {
        self.sparse_checkout_add(paths).await
    }

    async fn sparse_checkout_disable(&self) -> Result<(), GitError> {
        self.sparse_checkout_disable().await
    }

    async fn sparse_checkout_list(&self) -> Result<Vec<String>, GitError> {
        self.sparse_checkout_list().await
    }

    async fn pull(&self, remote: &str, branch: &str, rebase: bool) -> Result<(), GitError> {
        self.pull(remote, branch, rebase).await
    }

    async fn switch(&self, branch: &str, create: bool) -> Result<(), GitError> {
        self.switch(branch, create).await
    }

    async fn restore(
        &self,
        paths: &[&Path],
        staged: bool,
        source: Option<&str>,
    ) -> Result<(), GitError> {
        self.restore(paths, staged, source).await
    }

    async fn revert(&self, commits: &[&str], no_edit: bool) -> Result<(), GitError> {
        self.revert(commits, no_edit).await
    }

    async fn stash_drop(&self, index: Option<usize>) -> Result<(), GitError> {
        self.stash_drop(index).await
    }

    async fn stash_apply(&self, index: Option<usize>) -> Result<(), GitError> {
        self.stash_apply(index).await
    }

    async fn stash_show(&self, index: Option<usize>) -> Result<String, GitError> {
        self.stash_show(index).await
    }

    async fn remote_add(&self, name: &str, url: &str) -> Result<(), GitError> {
        self.remote_add(name, url).await
    }

    async fn remote_remove(&self, name: &str) -> Result<(), GitError> {
        self.remote_remove(name).await
    }

    async fn remote_rename(&self, old: &str, new: &str) -> Result<(), GitError> {
        self.remote_rename(old, new).await
    }

    async fn ls_remote(
        &self,
        remote: &str,
        refs: Option<&[&str]>,
    ) -> Result<Vec<(String, String)>, GitError> {
        self.ls_remote(remote, refs).await
    }

    async fn branch_rename(&self, old: &str, new: &str, force: bool) -> Result<(), GitError> {
        self.branch_rename(old, new, force).await
    }

    async fn tag_delete(&self, name: &str) -> Result<(), GitError> {
        self.tag_delete(name).await
    }

    async fn tag_create_signed(
        &self,
        name: &str,
        message: &str,
        gpg_key: Option<&str>,
    ) -> Result<(), GitError> {
        self.tag_create_signed(name, message, gpg_key).await
    }

    async fn verify_tag(&self, name: &str) -> Result<crate::types::GitVerification, GitError> {
        self.verify_tag(name).await
    }

    async fn worktree_prune(&self) -> Result<(), GitError> {
        self.worktree_prune().await
    }

    async fn worktree_lock(&self, path: &Path) -> Result<(), GitError> {
        self.worktree_lock(path).await
    }

    async fn worktree_unlock(&self, path: &Path) -> Result<(), GitError> {
        self.worktree_unlock(path).await
    }

    async fn worktree_move(&self, old_path: &Path, new_path: &Path) -> Result<(), GitError> {
        self.worktree_move(old_path, new_path).await
    }

    async fn mv(&self, source: &Path, dest: &Path) -> Result<(), GitError> {
        self.mv(source, dest).await
    }

    async fn rm(&self, paths: &[&Path], cached: bool) -> Result<(), GitError> {
        self.rm(paths, cached).await
    }

    async fn merge_base(&self, commits: &[&str]) -> Result<String, GitError> {
        self.merge_base(commits).await
    }

    async fn rev_parse(&self, rev: &str) -> Result<String, GitError> {
        self.rev_parse(rev).await
    }

    async fn hash_object(&self, data: &[u8]) -> Result<Oid, GitError> {
        self.hash_object(data).await
    }

    async fn write_blob(&self, data: &[u8]) -> Result<Oid, GitError> {
        self.write_blob(data).await
    }

    async fn mktree(&self, entries: &[TreeEntry]) -> Result<Oid, GitError> {
        self.mktree(entries).await
    }

    async fn write_tree(&self, entries: &[TreeEntry]) -> Result<Oid, GitError> {
        self.write_tree(entries).await
    }

    async fn read_object(&self, oid: &Oid) -> Result<ObjectContent, GitError> {
        self.read_object(oid).await
    }

    async fn read_tree(&self, oid: &Oid) -> Result<Vec<TreeEntry>, GitError> {
        self.read_tree(oid).await
    }

    async fn read_blob(&self, oid: &Oid) -> Result<Vec<u8>, GitError> {
        self.read_blob(oid).await
    }

    async fn read_commit(&self, oid: &Oid) -> Result<crate::types::GitCommit, GitError> {
        self.read_commit(oid).await
    }

    async fn write_commit(
        &self,
        parents: &[&Oid],
        tree: &Oid,
        message: &str,
    ) -> Result<Oid, GitError> {
        self.write_commit(parents, tree, message).await
    }

    async fn read_index(&self) -> Result<Vec<IndexEntry>, GitError> {
        self.read_index().await
    }

    async fn update_index(&self, path: &Path, oid: &Oid, mode: u32) -> Result<(), GitError> {
        self.update_index(path, oid, mode).await
    }

    async fn remove_index(&self, path: &Path) -> Result<(), GitError> {
        self.remove_index(path).await
    }
}
