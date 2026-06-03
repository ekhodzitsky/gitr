use std::path::PathBuf;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Status of a git working tree, parsed from porcelain output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitStatus {
    /// Files with staged changes.
    pub staged: Vec<String>,
    /// Files with unstaged changes.
    pub unstaged: Vec<String>,
    /// Untracked files.
    pub untracked: Vec<String>,
}

/// A single entry from `git log`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[allow(dead_code)]
pub struct GitLogEntry {
    /// Full commit SHA.
    pub sha: String,
    /// Short (7-character) commit SHA.
    pub short_sha: String,
    /// Commit message.
    pub message: String,
    /// Author name.
    pub author: String,
    /// Commit timestamp (Unix epoch seconds as string).
    pub timestamp: String,
}

/// A configured git remote.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[allow(dead_code)]
pub struct GitRemote {
    /// Remote name (e.g. "origin").
    pub name: String,
    /// Remote URL.
    pub url: String,
}

/// Result of a read-only merge-tree operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitMergeResult {
    /// Whether any conflicts were detected.
    pub has_conflicts: bool,
    /// Files with conflicts (empty if `has_conflicts` is false).
    pub conflict_files: Vec<String>,
    /// The resulting tree OID (if available).
    pub tree_oid: Option<String>,
}

/// A git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitWorktree {
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// Branch tracked by this worktree.
    pub branch: String,
}

/// A git tag.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitTag {
    /// Tag name.
    pub name: String,
    /// Tagged object SHA.
    pub sha: String,
    /// Tag message (empty for lightweight tags).
    pub message: String,
}

/// A single stash entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitStash {
    /// Stash reference (e.g. "stash@{0}").
    pub ref_name: String,
    /// Commit SHA.
    pub sha: String,
    /// Stash message.
    pub message: String,
}

/// Reset mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ResetMode {
    /// Mixed reset (default).
    Mixed,
    /// Soft reset.
    Soft,
    /// Hard reset.
    Hard,
}
