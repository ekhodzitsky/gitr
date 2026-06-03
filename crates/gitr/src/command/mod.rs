use crate::error::GitError;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

static GIT_BIN_PATH: LazyLock<Result<PathBuf, GitError>> =
    LazyLock::new(|| which::which("git").map_err(|_| GitError::GitNotFound));

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(any(test, feature = "test-utils"))]
mod scripted;

#[cfg(any(test, feature = "test-utils"))]
pub use scripted::ScriptedRunner;

const GIT_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RETRIES: u32 = 3;

/// Output of a finished git command.
///
/// `stderr` and `exit_code` are public so downstream consumers can inspect
/// them, but they are not read within this crate.
#[allow(dead_code)]
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CommandOutput {
    /// Standard output from the command.
    pub stdout: String,
    /// Standard error from the command.
    pub stderr: String,
    /// Process exit code.
    pub exit_code: i32,
}

/// Low-level git command runner with timeout, retry, and non-interactive env.
#[derive(Debug, Clone)]
pub struct GitCommand {
    cwd: PathBuf,
    git_bin: PathBuf,
    timeout: Duration,
    max_retries: u32,
    cancel: Option<CancellationToken>,
}

impl GitCommand {
    /// Create a new command runner for the given working directory.
    pub fn new(cwd: impl AsRef<Path>) -> Result<Self, GitError> {
        let cwd = cwd.as_ref().to_path_buf();
        let git_bin = GIT_BIN_PATH.clone()?;
        Ok(Self {
            cwd,
            git_bin,
            timeout: GIT_TIMEOUT,
            max_retries: MAX_RETRIES,
            cancel: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn new_with_git_bin(cwd: PathBuf, git_bin: PathBuf) -> Self {
        Self {
            cwd,
            git_bin,
            timeout: GIT_TIMEOUT,
            max_retries: MAX_RETRIES,
            cancel: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Attach a cancellation token to this command runner.
    ///
    /// When the token is cancelled, any in-flight git command is killed
    /// and [`GitError::Io`] is returned.
    #[allow(dead_code)]
    pub fn with_cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Run a git command with the given arguments.
    pub async fn run(&self, args: &[impl AsRef<OsStr>]) -> Result<CommandOutput, GitError> {
        self.run_with_env(args, &[]).await
    }

    /// Run a git command with extra environment variables.
    pub async fn run_with_env(
        &self,
        args: &[impl AsRef<OsStr>],
        extra_env: &[(&str, &str)],
    ) -> Result<CommandOutput, GitError> {
        let command_str = args
            .iter()
            .map(|a| a.as_ref().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let mut attempt = 0;

        loop {
            if let Some(c) = &self.cancel {
                if c.is_cancelled() {
                    return Err(GitError::Io("cancelled".to_string()));
                }
            }

            let mut cmd = Command::new(&self.git_bin);
            cmd.args(args)
                .current_dir(&self.cwd)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_ASKPASS", "echo")
                .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
                .env("LC_ALL", "C")
                .kill_on_drop(true);

            for (k, v) in extra_env {
                cmd.env(k, v);
            }

            let result = if let Some(c) = &self.cancel {
                let output_fut = cmd.output();
                tokio::pin!(output_fut);
                tokio::select! {
                    biased;
                    _ = c.cancelled() => {
                        return Err(GitError::Io("cancelled".to_string()));
                    }
                    r = tokio::time::timeout(self.timeout, output_fut) => r,
                }
            } else {
                let output_fut = cmd.output();
                tokio::pin!(output_fut);
                tokio::time::timeout(self.timeout, output_fut).await
            };

            match result {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                    let status = output.status;

                    let exit_code = status.code().unwrap_or(-1);
                    if status.success() {
                        return Ok(CommandOutput {
                            stdout,
                            stderr,
                            exit_code,
                        });
                    }
                    if attempt < self.max_retries && is_retryable(&stderr) {
                        attempt += 1;
                        let backoff = Duration::from_millis(100 * 2_u64.pow(attempt - 1));
                        sleep(backoff).await;
                        continue;
                    }

                    return Err(GitError::CommandFailed {
                        command: command_str,
                        exit_code,
                        stderr,
                        stdout,
                    });
                }
                Ok(Err(e)) => {
                    return Err(GitError::Io(format!("failed to run git: {e}")));
                }
                Err(_) => {
                    return Err(GitError::Timeout(self.timeout, command_str));
                }
            }
        }
    }
}

fn is_retryable(stderr: &str) -> bool {
    let needle = stderr.to_lowercase();
    needle.contains("unable to access")
        || needle.contains("timeout")
        || needle.contains("early eof")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_is_retryable_true() {
        assert!(is_retryable(
            "fatal: unable to access 'https://...': early EOF"
        ));
        assert!(is_retryable("network timeout"));
        assert!(is_retryable("unable to access"));
    }

    #[test]
    fn test_is_retryable_false() {
        assert!(!is_retryable("fatal: not a git repository"));
        assert!(!is_retryable("error: pathspec 'foo' did not match"));
    }

    fn write_script(path: &std::path::Path, content: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(path, perms).unwrap();
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_command_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("slow-git");
        write_script(&script, "#!/bin/sh\nsleep 2\n");
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_timeout(Duration::from_millis(100));
        let err = cmd.run(&["status"]).await.unwrap_err();
        assert!(matches!(err, GitError::Timeout(_, _)));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_retry_on_network_error() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("flaky-git");
        let counter = tmp.path().join("counter");
        write_script(
            &script,
            &format!(
                "#!/bin/sh\n\
                count=$(cat '{}' 2>/dev/null || echo 0)\n\
                echo $((count + 1)) > '{}'\n\
                echo 'fatal: unable to access: early EOF' >&2\n\
                exit 1\n",
                counter.display(),
                counter.display()
            ),
        );
        let cmd =
            GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script).with_max_retries(2);
        let err = cmd.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(matches!(err, GitError::CommandFailed { .. }));
        let count = std::fs::read_to_string(&counter)
            .unwrap()
            .trim()
            .parse::<u32>()
            .unwrap();
        assert_eq!(count, 3); // initial + 2 retries
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_command_output_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(
            &script,
            "#!/bin/sh\necho stdout\necho stderr >&2\nexit 42\n",
        );
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script);
        let out = cmd.run(&["status"]).await.unwrap_err();
        if let GitError::CommandFailed {
            command,
            exit_code,
            stderr,
            stdout,
        } = out
        {
            assert_eq!(command, "status");
            assert_eq!(exit_code, 42);
            assert_eq!(stdout.trim(), "stdout");
            assert_eq!(stderr.trim(), "stderr");
        } else {
            panic!("expected CommandFailed");
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cancellation() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("slow-git");
        write_script(&script, "#!/bin/sh\nsleep 10\n");
        let cancel = CancellationToken::new();
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_cancel(cancel.clone());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel.cancel();
        });
        let err = cmd.run(&["status"]).await.unwrap_err();
        assert!(matches!(err, GitError::Io(ref s) if s == "cancelled"));
    }
}
