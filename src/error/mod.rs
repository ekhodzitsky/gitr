use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

/// Errors that can occur when interacting with git repositories.
#[derive(Debug, Error, Clone)]
pub enum GitError {
    /// The path is not a git repository.
    #[error("not a git repository: {0}")]
    NotARepo(PathBuf),

    /// The `git` binary was not found in PATH.
    #[error("git not found in PATH")]
    GitNotFound,

    /// A git command failed with a non-zero exit code.
    #[error("command failed: {command} — exit {exit_code}, stderr: {stderr}")]
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr: String,
        stdout: String,
    },

    /// The command timed out.
    #[error("command timed out after {0:?}: {1}")]
    Timeout(Duration, String),

    /// The working tree is not clean.
    #[error("repository is not clean: {0}")]
    Dirty(String),

    /// The branch already exists.
    #[error("branch already exists: {0}")]
    BranchExists(String),

    /// The branch was not found.
    #[error("branch not found: {0}")]
    BranchNotFound(String),

    /// The worktree already exists.
    #[error("worktree already exists: {0}")]
    WorktreeExists(String),

    /// Merge conflicts were detected.
    #[error("merge conflicts detected")]
    MergeConflicts(Vec<String>),

    /// An I/O error occurred.
    #[error("io error: {0}")]
    Io(String),

    /// Failed to parse git output.
    #[error("parse error: {0}")]
    Parse(String),
}

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        GitError::Io(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let git_err: GitError = io_err.into();
        assert!(matches!(git_err, GitError::Io(ref s) if s.contains("file not found")));
    }
}
