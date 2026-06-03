use std::path::PathBuf;

/// Status of a git working tree, parsed from porcelain output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    pub staged: Vec<String>,
    pub unstaged: Vec<String>,
    pub untracked: Vec<String>,
}

/// A single entry from `git log`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct GitLogEntry {
    pub sha: String,
    pub short_sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
}

/// A configured git remote.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
}

/// Result of a read-only merge-tree operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitMergeResult {
    pub has_conflicts: bool,
    pub conflict_files: Vec<String>,
    pub tree_oid: Option<String>,
}

/// A git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktree {
    pub path: PathBuf,
    pub branch: String,
}
