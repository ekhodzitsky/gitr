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

    /// Authentication is required but no credentials were provided.
    #[error("authentication required")]
    AuthenticationRequired,

    /// A network error occurred.
    #[error("network error: {0}")]
    NetworkError(String),

    /// A git hook rejected the operation.
    #[error("hook rejected: {0}")]
    HookRejected(String),

    /// A rebase is already in progress.
    #[error("rebase in progress")]
    RebaseInProgress,

    /// A merge is already in progress.
    #[error("merge in progress")]
    MergeInProgress,

    /// A cherry-pick is already in progress.
    #[error("cherry-pick in progress")]
    CherryPickInProgress,

    /// Nothing to commit.
    #[error("nothing to commit")]
    NothingToCommit,

    /// Unmerged paths prevent the operation.
    #[error("unmerged paths")]
    UnmergedPaths(Vec<String>),

    /// The remote was not found.
    #[error("remote not found: {0}")]
    RemoteNotFound(String),

    /// The tag was not found.
    #[error("tag not found: {0}")]
    TagNotFound(String),
}

impl GitError {
    /// Classify a failed command into the most specific error variant.
    pub fn classify(command: String, exit_code: i32, stderr: String, stdout: String) -> Self {
        let lower = stderr.to_lowercase();
        if lower.contains("authentication failed")
            || lower.contains("could not read username")
            || lower.contains("terminal prompts disabled")
        {
            return GitError::AuthenticationRequired;
        }
        if lower.contains("could not resolve host")
            || lower.contains("unable to access")
            || lower.contains("failed to connect")
        {
            return GitError::NetworkError(stderr.clone());
        }
        if lower.contains("hook exit code") || lower.contains("pre-commit hook") {
            return GitError::HookRejected(stderr.clone());
        }
        if lower.contains("rebase in progress") || lower.contains(".git/rebase-apply") {
            return GitError::RebaseInProgress;
        }
        if lower.contains("merge in progress") || lower.contains(".git/merge_head") {
            return GitError::MergeInProgress;
        }
        if lower.contains("cherry-pick in progress") || lower.contains(".git/sequencer") {
            return GitError::CherryPickInProgress;
        }
        if lower.contains("nothing to commit") {
            return GitError::NothingToCommit;
        }
        if lower.contains("unmerged paths") || lower.contains("is unmerged") {
            return GitError::UnmergedPaths(vec![]);
        }
        if lower.contains("no such remote") {
            return GitError::RemoteNotFound(stderr.lines().next().unwrap_or(&stderr).to_string());
        }
        if lower.contains("tag not found") || lower.contains("did not match any tag") {
            return GitError::TagNotFound(stderr.lines().next().unwrap_or(&stderr).to_string());
        }
        GitError::CommandFailed {
            command,
            exit_code,
            stderr,
            stdout,
        }
    }

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
            }
            GitError::Timeout(..) => true,
            GitError::Io(msg) => {
                let needle = msg.to_lowercase();
                needle.contains("connection refused") || needle.contains("timed out")
            }
            GitError::NetworkError(_) => true,
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

    #[test]
    fn test_is_retryable_io_connection_refused() {
        let e = GitError::Io("connection refused".to_string());
        assert!(e.is_retryable());

        let e = GitError::Io("timed out".to_string());
        assert!(e.is_retryable());

        let e = GitError::Io("file not found".to_string());
        assert!(!e.is_retryable());
    }

    #[test]
    fn test_is_retryable_network_error() {
        let e = GitError::NetworkError("dns failure".to_string());
        assert!(e.is_retryable());
    }

    #[test]
    fn test_classify_authentication_required() {
        let e = GitError::classify(
            "push".to_string(),
            1,
            "fatal: authentication failed for https://example.com\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::AuthenticationRequired));
    }

    #[test]
    fn test_classify_network_error() {
        let e = GitError::classify(
            "fetch".to_string(),
            1,
            "fatal: could not resolve host: example.com\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::NetworkError(ref s) if s.contains("could not resolve")));
    }

    #[test]
    fn test_classify_hook_rejected() {
        let e = GitError::classify(
            "commit".to_string(),
            1,
            "pre-commit hook exited with code 1\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::HookRejected(ref s) if s.contains("pre-commit")));
    }

    #[test]
    fn test_classify_rebase_in_progress() {
        let e = GitError::classify(
            "merge".to_string(),
            1,
            "rebase in progress\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::RebaseInProgress));
    }

    #[test]
    fn test_classify_merge_in_progress() {
        let e = GitError::classify(
            "checkout".to_string(),
            1,
            "merge in progress\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::MergeInProgress));
    }

    #[test]
    fn test_classify_cherry_pick_in_progress() {
        let e = GitError::classify(
            "commit".to_string(),
            1,
            "cherry-pick in progress\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::CherryPickInProgress));
    }

    #[test]
    fn test_classify_nothing_to_commit() {
        let e = GitError::classify(
            "commit".to_string(),
            1,
            "nothing to commit, working tree clean\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::NothingToCommit));
    }

    #[test]
    fn test_classify_unmerged_paths() {
        let e = GitError::classify(
            "commit".to_string(),
            1,
            "error: unmerged paths\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::UnmergedPaths(ref v) if v.is_empty()));
    }

    #[test]
    fn test_classify_remote_not_found() {
        let e = GitError::classify(
            "fetch".to_string(),
            1,
            "fatal: no such remote 'upstream'\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::RemoteNotFound(ref s) if s.contains("upstream")));
    }

    #[test]
    fn test_classify_tag_not_found() {
        // "did not match any tag" is the pattern classify looks for
        let e = GitError::classify(
            "show".to_string(),
            1,
            "error: pathspec 'v99' did not match any tag(s) known to git\n".to_string(),
            String::new(),
        );
        assert!(matches!(e, GitError::TagNotFound(ref s) if s.contains("v99")));
    }

    #[test]
    fn test_classify_fallback_command_failed() {
        let e = GitError::classify(
            "status".to_string(),
            128,
            "fatal: not a git repository\n".to_string(),
            String::new(),
        );
        assert!(
            matches!(e, GitError::CommandFailed { ref command, exit_code: 128, .. } if command == "status")
        );
    }
}
