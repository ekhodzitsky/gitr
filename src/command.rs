use crate::error::GitError;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Output of a git command.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Low-level git command runner.
#[derive(Debug, Clone)]
pub struct GitCommand {
    cwd: PathBuf,
    git_bin: PathBuf,
}

impl GitCommand {
    /// Create a new command runner for the given working directory.
    pub fn new(cwd: impl AsRef<Path>) -> Result<Self, GitError> {
        let cwd = cwd.as_ref().to_path_buf();
        let git_bin = which::which("git").map_err(|_| GitError::GitNotFound)?;
        Ok(Self { cwd, git_bin })
    }

    /// Run a git command with the given arguments.
    pub async fn run(&self, args: &[&str]) -> Result<CommandOutput, GitError> {
        self.run_with_timeout(args, DEFAULT_TIMEOUT).await
    }

    /// Run a git command with a custom timeout.
    pub async fn run_with_timeout(
        &self,
        args: &[&str],
        _dur: Duration,
    ) -> Result<CommandOutput, GitError> {
        let mut cmd = Command::new(&self.git_bin);
        cmd.current_dir(&self.cwd).args(args).kill_on_drop(true);

        #[cfg(feature = "tracing")]
        tracing::debug!(cmd = %format!("git {}", args.join(" ")), "spawning git");

        let child = cmd
            .output()
            .await
            .map_err(|e| GitError::Io(format!("failed to spawn git: {e}")))?;

        let stdout = String::from_utf8_lossy(&child.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&child.stderr).into_owned();
        let exit_code = child.status.code().unwrap_or(-1);

        if exit_code != 0 {
            return Err(GitError::CommandFailed {
                command: format!("git {}", args.join(" ")),
                exit_code,
                stderr,
                stdout,
            });
        }

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
        })
    }

    /// Run with environment variables.
    pub async fn run_with_env(
        &self,
        args: &[&str],
        envs: &[(&str, &str)],
    ) -> Result<CommandOutput, GitError> {
        let mut cmd = Command::new(&self.git_bin);
        cmd.current_dir(&self.cwd).args(args).kill_on_drop(true);
        for (k, v) in envs {
            cmd.env(k, v);
        }

        let child = cmd
            .output()
            .await
            .map_err(|e| GitError::Io(format!("failed to spawn git: {e}")))?;

        let stdout = String::from_utf8_lossy(&child.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&child.stderr).into_owned();
        let exit_code = child.status.code().unwrap_or(-1);

        if exit_code != 0 {
            return Err(GitError::CommandFailed {
                command: format!("git {}", args.join(" ")),
                exit_code,
                stderr,
                stdout,
            });
        }

        Ok(CommandOutput {
            stdout,
            stderr,
            exit_code,
        })
    }
}
