# gitr Specification

## Purpose

`gitr` is an async typed git CLI wrapper optimized for AI agents and automation.
It shells out to the `git` binary and provides structured types, typed errors,
and porcelain parsing — no C dependencies, no FFI, pure Rust.

## Design Decisions

### Why CLI over libgit2/gitoxide

- `git2` = sync FFI to libgit2. Requires `spawn_blocking` for async, heavy C deps.
- `gix` (gitoxide) = pure Rust but lacks rebase/merge/stash/checkout orchestration.
- CLI subprocess is the only approach with **full feature coverage**, **zero C dependencies**,
  and **native async** via `tokio::process::Command`.

### Why `which::which("git")` at init time

Discovering the git binary once at `Repository::open` avoids per-call `PATH` lookups
and allows early failure with `GitError::GitNotFound`.

### Why `thiserror` not `anyhow`

`gitr` is a library. Callers must be able to `match` on `GitError::BranchNotFound`
to implement fallback logic. `anyhow` is banned from the public API.

## Public API Contract

### `Repository` — Primary Interface

```rust
pub struct Repository { /* private fields */ }

impl Repository {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, GitError>;
    pub fn root(&self) -> &Path;

    // Status
    pub async fn ensure_clean(&self) -> Result<(), GitError>;
    pub async fn status(&self) -> Result<GitStatus, GitError>;
    pub async fn changed_files(&self) -> Result<Vec<String>, GitError>;
    pub async fn conflicted_files(&self) -> Result<Vec<String>, GitError>;

    // Branch
    pub async fn current_branch(&self) -> Result<String, GitError>;
    pub async fn branch_create(&self, name: &str, start_point: Option<&str>) -> Result<(), GitError>;
    pub async fn branch_delete(&self, name: &str, force: bool) -> Result<(), GitError>;
    pub async fn branch_exists(&self, name: &str) -> Result<bool, GitError>;
    pub async fn checkout(&self, branch: &str) -> Result<(), GitError>;
    pub async fn default_branch(&self) -> Result<String, GitError>;

    // Commit & Push
    pub async fn commit(&self, message: &str, paths: &[impl AsRef<Path>], no_verify: bool) -> Result<String, GitError>;
    pub async fn commit_signed(&self, paths: &[&Path], message: &str, gpg_key: Option<&str>, no_verify: bool) -> Result<(), GitError>;
    pub async fn verify_commit(&self, sha: &str) -> Result<GitVerification, GitError>;
    pub async fn push(&self, remote: &str, branch: &str, force: bool) -> Result<(), GitError>;
    pub async fn fetch(&self, remote: &str) -> Result<(), GitError>;
    pub async fn remote_url(&self, remote: &str) -> Result<Option<String>, GitError>;

    // Worktree
    pub async fn worktree_add(&self, path: impl AsRef<Path>, branch: &str) -> Result<GitWorktree, GitError>;
    pub async fn worktree_remove(&self, path: impl AsRef<Path>, force: bool) -> Result<(), GitError>;
    pub async fn worktree_list(&self) -> Result<Vec<GitWorktree>, GitError>;

    // Merge & Rebase
    pub async fn merge(&self, branch: &str, no_edit: bool) -> Result<(), GitError>;
    pub async fn merge_tree(&self, base: &str, branch: &str) -> Result<GitMergeResult, GitError>;
    pub async fn rebase(&self, branch: &str) -> Result<(), GitError>;
    pub async fn rebase_continue(&self) -> Result<(), GitError>;
    pub async fn rebase_abort(&self) -> Result<(), GitError>;

    // Stash
    pub async fn stash(&self, message: Option<&str>) -> Result<(), GitError>;
    pub async fn stash_pop(&self) -> Result<(), GitError>;

    // Diff
    pub async fn diff(&self) -> Result<String, GitError>;
    pub async fn diff_cached(&self) -> Result<String, GitError>;
    pub async fn add(&self, path: impl AsRef<Path>) -> Result<(), GitError>;
    pub async fn add_all(&self) -> Result<(), GitError>;

    // Config
    pub async fn config_get(&self, key: &str) -> Result<Option<String>, GitError>;
    pub async fn config_set(&self, key: &str, value: &str) -> Result<(), GitError>;

    // Tag
    pub async fn tag_list(&self) -> Result<Vec<GitTag>, GitError>;
    pub async fn tag_create(&self, name: &str, message: Option<&str>, force: bool) -> Result<(), GitError>;

    // Submodule
    pub async fn submodule_list(&self) -> Result<Vec<GitSubmodule>, GitError>;
    pub async fn submodule_add(&self, url: &str, path: impl AsRef<Path>) -> Result<(), GitError>;
    pub async fn submodule_update(&self, init: bool, recursive: bool) -> Result<(), GitError>;
    pub async fn submodule_deinit(&self, path: impl AsRef<Path>, force: bool) -> Result<(), GitError>;
    pub async fn submodule_sync(&self) -> Result<(), GitError>;

    // Query
    pub async fn show(&self, path: &str, rev: Option<&str>) -> Result<String, GitError>;
    pub async fn blame(&self, path: &str) -> Result<String, GitError>;
    pub async fn grep(&self, pattern: &str) -> Result<Vec<GitGrepResult>, GitError>;
    pub async fn ls_files(&self, deleted: bool, others: bool, exclude_standard: bool) -> Result<Vec<String>, GitError>;
    pub async fn log(&self, max_count: Option<usize>) -> Result<Vec<GitLogEntry>, GitError>;
    pub async fn log_paginated(&self, skip: usize, max_count: usize) -> Result<Vec<GitLogEntry>, GitError>;
    pub async fn describe(&self, tags: bool, long: bool) -> Result<String, GitError>;
    pub async fn clean(&self, force: bool, directories: bool, dry_run: bool) -> Result<Vec<String>, GitError>;

    // Reset & cherry-pick
    pub async fn reset(&self, mode: ResetMode, target: Option<&str>) -> Result<(), GitError>;
    pub async fn cherry_pick(&self, commits: &[&str]) -> Result<(), GitError>;
    pub async fn stash_list(&self) -> Result<Vec<GitStash>, GitError>;

    // Constructors & helpers
    pub async fn init(path: impl AsRef<Path>) -> Result<Self, GitError>;
    pub async fn clone(url: &str, path: impl AsRef<Path>) -> Result<Self, GitError>;
    pub fn with_cache(self, cache: Cache) -> Self;
    pub fn with_cancel(self, cancel: CancellationToken) -> Self;
    pub fn with_timeout(self, timeout: Duration) -> Self;
}
```

### Error Types

```rust
pub enum GitError {
    NotARepo(PathBuf),
    GitNotFound,
    CommandFailed { command: String, exit_code: i32, stderr: String, stdout: String },
    Timeout(Duration, String),
    Dirty(String),
    BranchExists(String),
    BranchNotFound(String),
    WorktreeExists(String),
    MergeConflicts(Vec<String>),
    Io(String),
    Parse(String),
}
```

### Data Types

```rust
pub struct GitStatus {
    pub staged: Vec<String>,
    pub unstaged: Vec<String>,
    pub untracked: Vec<String>,
}

pub struct GitMergeResult {
    pub has_conflicts: bool,
    pub conflict_files: Vec<String>,
    pub tree_oid: Option<String>,
}

pub struct GitWorktree {
    pub path: PathBuf,
    pub branch: String,
}

pub struct GitSubmodule {
    pub sha: String,
    pub path: String,
    pub describe: Option<String>,
    pub dirty: bool,
    pub uninitialized: bool,
}

pub struct GitVerification {
    pub valid: bool,
    pub signer: Option<String>,
    pub fingerprint: Option<String>,
    pub status: String,
}

pub struct GitGrepResult {
    pub path: String,
    pub line: u32,
    pub text: String,
}

pub struct GitLogEntry {
    pub sha: String,
    pub short_sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
}

pub struct GitTag {
    pub name: String,
    pub sha: String,
    pub message: String,
}

pub struct GitStash {
    pub ref_name: String,
    pub sha: String,
    pub message: String,
}

pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

pub struct DiffShortstat {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

pub enum DiffLineKind {
    Context,
    Deletion,
    Insertion,
    NoNewline,
}

pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
}

pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub section: String,
    pub lines: Vec<DiffLine>,
}

pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub is_binary: bool,
    pub mode_changed: bool,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub hunks: Vec<DiffHunk>,
}

pub struct BlameLine {
    pub commit: String,
    pub author: String,
    pub author_mail: String,
    pub author_time: String,
    pub line_no: usize,
    pub content: String,
}

pub struct Patch {
    pub commit: Option<String>,
    pub subject: Option<String>,
    pub from: Option<String>,
    pub date: Option<String>,
    pub diff: Vec<FileDiff>,
}

pub struct ApplyReport {
    pub files_changed: Vec<String>,
}

pub struct ReflogEntry {
    pub commit: String,
    pub author: String,
    pub author_mail: String,
    pub timestamp: String,
    pub subject: String,
    pub designator: String,
}

pub struct Hook {
    pub name: String,
    pub path: PathBuf,
    pub active: bool,
}

pub struct HookOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub struct BisectResult {
    pub found: Option<String>,
    pub remaining: usize,
    pub log: Vec<String>,
}

pub struct BisectState {
    pub current: Option<String>,
}

pub struct GitNote {
    pub object: String,
    pub commit: String,
}

pub struct GitAttr {
    pub path: String,
    pub attr: String,
    pub value: String,
}

pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

pub struct PushOptions<'a> {
    pub remote: &'a str,
    pub branch: &'a str,
    pub force: bool,
    pub force_with_lease: bool,
    pub set_upstream: bool,
}

pub struct CommitOptions<'a> {
    pub message: &'a str,
    pub paths: &'a [&'a Path],
    pub no_verify: bool,
    pub amend: bool,
    pub signoff: bool,
}

pub struct RebaseOptions<'a> {
    pub branch: &'a str,
    pub interactive: bool,
    pub autosquash: bool,
    pub onto: Option<&'a str>,
}

pub struct MergeOptions<'a> {
    pub branch: &'a str,
    pub no_edit: bool,
    pub no_ff: bool,
    pub squash: bool,
}

pub struct FetchOptions<'a> {
    pub remote: &'a str,
    pub prune: bool,
    pub tags: bool,
    pub depth: Option<usize>,
}

pub struct CherryPickOptions<'a> {
    pub commits: &'a [&'a str],
    pub no_commit: bool,
}

pub struct TreeEntry {
    pub mode: String,
    pub path: String,
    pub oid: String,
}

pub struct IndexEntry {
    pub path: String,
    pub oid: String,
    pub mode: u32,
}

pub struct ObjectContent {
    pub oid: String,
    pub kind: String,
    pub size: usize,
    pub data: Vec<u8>,
}

pub struct GitCommit {
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub committer: String,
    pub message: String,
}

pub struct GitLfsFile {
    pub oid: String,
    pub path: String,
    pub size: Option<u64>,
}
```

## Git CLI Compatibility Matrix

| Operation | Git Command | Machine-Readable Flag | Parser |
|---|---|---|---|
| Status | `status --porcelain` | `--porcelain` | `parse_status` |
| Worktree list | `worktree list --porcelain` | `--porcelain` | `parse_worktrees` |
| Branch list | `branch --format=%(refname:short)` | `--format` | `parse_branches` |
| Merge-tree | `merge-tree <base> <branch>` | N/A | `parse_merge_tree` |
| Diff | `diff` (unified format) | N/A | `parse_diff` |

## Feature Flags

| Feature | Default | Description |
|---|---|---|
| `tracing` | ✅ | Emit `tracing` spans for command execution. |
| `serde` | ❌ | Derive `Serialize`/`Deserialize` on public types. |
| `stream` | ❌ | Provide `log_stream()`, `grep_stream()`, `blame_stream()`, `ls_files_stream()` returning `impl Stream`. |
| `metrics` | ❌ | Emit `metrics` counters and histograms for command execution. |
| `test-utils` | ❌ | Internal helpers for unit tests (no public API). |

## MSRV

Rust **1.80**.

## Future Directions

- `ScriptedRunner`/`RecordingRunner` for hermetic tests
- Agent-specific convenience methods
