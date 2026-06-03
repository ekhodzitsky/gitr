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
        /// The full command string that was executed.
        command: String,
        /// The process exit code.
        exit_code: i32,
        /// Standard error captured from the command.
        stderr: String,
        /// Standard output captured from the command.
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

impl GitError {
    /// Whether this error represents a transient (retryable) condition.
    ///
    /// Network timeouts, early EOF, and access failures are considered
    /// transient and may succeed on retry.
    pub fn is_retryable(&self) -> bool {
        match self {
            GitError::CommandFailed { stderr, .. } => {
                let needle = stderr.to_lowercase();
                needle.contains("unable to access")
                    || needle.contains("timeout")
                    || needle.contains("early eof")
                    || needle.contains("fatal: unable to access")
            }
            GitError::Timeout(..) => true,
            GitError::Io(msg) => {
                let needle = msg.to_lowercase();
                needle.contains("connection refused") || needle.contains("timed out")
            }
            _ => false,
        }
    }
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

    #[test]
    fn test_is_retryable_true() {
        let e = GitError::CommandFailed {
            command: "fetch".to_string(),
            exit_code: 1,
            stderr: "fatal: unable to access: early EOF".to_string(),
            stdout: String::new(),
        };
        assert!(e.is_retryable());

        let e = GitError::Timeout(Duration::from_secs(10), "fetch".to_string());
        assert!(e.is_retryable());
    }

    #[test]
    fn test_is_retryable_false() {
        let e = GitError::NotARepo(std::path::PathBuf::from("/tmp"));
        assert!(!e.is_retryable());

        let e = GitError::CommandFailed {
            command: "status".to_string(),
            exit_code: 1,
            stderr: "not a git repository".to_string(),
            stdout: String::new(),
        };
        assert!(!e.is_retryable());
    }
}
