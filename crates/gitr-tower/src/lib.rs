//! Tower Service integration for `gitr`.
//!
//! Provides a [`tower::Service`] implementation over git operations,
//! enabling middleware such as retries, timeouts, rate limiting, and load
//! balancing.
//!
//! ```no_run
//! use gitr_tower::GitService;
//! use tower::{Service, ServiceExt};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut svc = GitService::new(".").await?;
//! let status = svc.ready().await?.call(gitr_tower::GitRequest::Status).await?;
//! # Ok(())
//! # }
//! ```

use futures::future::BoxFuture;
use gitr::Repository;
use std::path::{Path, PathBuf};
use std::task::{Context, Poll};

/// A request to perform a git operation.
#[derive(Debug, Clone)]
pub enum GitRequest {
    /// Run `git status`.
    Status,
    /// Run `git log` with optional limit.
    Log { max_count: Option<usize> },
    /// Run `git fetch` from the given remote.
    Fetch { remote: String },
    /// Run `git push` to the given remote and branch.
    Push { remote: String, branch: String },
    /// Get the current branch name.
    CurrentBranch,
}

/// Response from a [`GitRequest`].
#[derive(Debug, Clone)]
pub enum GitResponse {
    /// Structured status.
    Status(gitr::GitStatus),
    /// Commit log entries.
    Log(Vec<gitr::GitLogEntry>),
    /// Fetch completed.
    Fetch,
    /// Push completed.
    Push,
    /// Current branch name.
    CurrentBranch(String),
}

/// Tower Service wrapper around [`Repository`].
#[derive(Debug, Clone)]
pub struct GitService {
    repo: Repository,
    cwd: PathBuf,
}

impl GitService {
    /// Open a repository at `path`.
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, gitr::Error> {
        let path = path.as_ref().to_path_buf();
        let repo = Repository::open(&path).await?;
        Ok(Self { repo, cwd: path })
    }

    /// Re-open the same repository (useful after clone).
    pub async fn reopen(&self) -> Result<Self, gitr::Error> {
        Self::new(&self.cwd).await
    }
}

impl tower::Service<GitRequest> for GitService {
    type Response = GitResponse;
    type Error = gitr::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: GitRequest) -> Self::Future {
        let repo = self.repo.clone();
        Box::pin(async move {
            match req {
                GitRequest::Status => {
                    let s = repo.status().await?;
                    Ok(GitResponse::Status(s))
                }
                GitRequest::Log { max_count } => {
                    let entries = repo.log(max_count).await?;
                    Ok(GitResponse::Log(entries))
                }
                GitRequest::Fetch { remote } => {
                    repo.fetch(&remote).await?;
                    Ok(GitResponse::Fetch)
                }
                GitRequest::Push { remote, branch } => {
                    repo.push(&remote, &branch, false).await?;
                    Ok(GitResponse::Push)
                }
                GitRequest::CurrentBranch => {
                    let b = repo.current_branch().await?;
                    Ok(GitResponse::CurrentBranch(b))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tower::{Service, ServiceExt};

    fn init_repo(dir: &std::path::Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .status()
            .unwrap();
        std::fs::write(dir.join("file.txt"), "hello").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args(["branch", "-m", "main"])
            .current_dir(dir)
            .status()
            .unwrap();
    }

    #[tokio::test]
    async fn test_service_status() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let mut svc = GitService::new(tmp.path()).await.unwrap();
        let resp = svc.call(GitRequest::Status).await.unwrap();
        assert!(matches!(resp, GitResponse::Status(_)));
    }

    #[tokio::test]
    async fn test_service_current_branch() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let mut svc = GitService::new(tmp.path()).await.unwrap();
        let resp = svc.call(GitRequest::CurrentBranch).await.unwrap();
        assert!(matches!(resp, GitResponse::CurrentBranch(ref b) if b == "main"));
    }

    #[tokio::test]
    async fn test_service_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let svc = GitService::new(tmp.path()).await.unwrap();
        let mut svc2 = svc.reopen().await.unwrap();
        let resp = svc2.call(GitRequest::CurrentBranch).await.unwrap();
        assert!(matches!(resp, GitResponse::CurrentBranch(ref b) if b == "main"));
    }

    #[tokio::test]
    async fn test_service_ready() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let mut svc = GitService::new(tmp.path()).await.unwrap();
        let ready = svc.ready().await;
        assert!(ready.is_ok());
    }

    #[tokio::test]
    async fn test_service_log() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let mut svc = GitService::new(tmp.path()).await.unwrap();
        let resp = svc
            .call(GitRequest::Log { max_count: Some(1) })
            .await
            .unwrap();
        assert!(matches!(resp, GitResponse::Log(ref entries) if entries.len() == 1));
    }

    #[tokio::test]
    async fn test_service_fetch_and_push() {
        let remote_tmp = tempfile::tempdir().unwrap();
        init_repo(remote_tmp.path());
        // Make remote_tmp a bare repo so we can push/fetch
        std::fs::remove_dir_all(remote_tmp.path().join(".git")).unwrap();
        std::process::Command::new("git")
            .args(["init", "--bare"])
            .current_dir(remote_tmp.path())
            .status()
            .unwrap();

        let local_tmp = tempfile::tempdir().unwrap();
        init_repo(local_tmp.path());
        std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                remote_tmp.path().to_str().unwrap(),
            ])
            .current_dir(local_tmp.path())
            .status()
            .unwrap();

        let mut svc = GitService::new(local_tmp.path()).await.unwrap();
        let resp = svc
            .call(GitRequest::Fetch {
                remote: "origin".to_string(),
            })
            .await
            .unwrap();
        assert!(matches!(resp, GitResponse::Fetch));

        let resp = svc
            .call(GitRequest::Push {
                remote: "origin".to_string(),
                branch: "main".to_string(),
            })
            .await
            .unwrap();
        assert!(matches!(resp, GitResponse::Push));
    }
}
