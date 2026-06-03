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
    pub async fn commit(&self, message: &str, paths: &[impl AsRef<Path>]) -> Result<String, GitError>;
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
    pub async fn add(&self, path: impl AsRef<Path>) -> Result<(), GitError>;
    pub async fn add_all(&self) -> Result<(), GitError>;
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
```

## Git CLI Compatibility Matrix

| Operation | Git Command | Machine-Readable Flag | Parser |
|---|---|---|---|
| Status | `status --porcelain` | `--porcelain` | `parse_status` |
| Worktree list | `worktree list --porcelain` | `--porcelain` | `parse_worktrees` |
| Branch list | `branch --format=%(refname:short)` | `--format` | `parse_branches` |
| Merge-tree | `merge-tree <base> <branch>` | N/A | `parse_merge_tree` |

## Feature Flags

| Feature | Default | Description |
|---|---|---|
| `tracing` | ✅ | Emit `tracing` spans for command execution. |

## MSRV

Rust **1.80**.

## Future Directions

- `GitApi` trait for mockability
- `ScriptedRunner`/`RecordingRunner` for hermetic tests
- Additional porcelain parsers (`diff --shortstat`, `log --format`)
- Agent-specific convenience methods
