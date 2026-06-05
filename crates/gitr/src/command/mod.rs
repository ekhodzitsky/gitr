use crate::error::GitError;
use crate::types::GitVersion;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

static GIT_BIN_PATH: LazyLock<Result<PathBuf, GitError>> =
    LazyLock::new(|| which::which("git").map_err(|_| GitError::GitNotFound));

static GIT_VERSION: LazyLock<Result<GitVersion, GitError>> = LazyLock::new(|| {
    let git_bin = git_bin_path()?;
    let output = std::process::Command::new(&git_bin)
        .arg("--version")
        .output()
        .map_err(|e| GitError::Io(format!("failed to run git --version: {e}")))?;
    if !output.status.success() {
        return Err(GitError::CommandFailed {
            command: "git --version".to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    GitVersion::parse(&stdout)
});

pub(crate) fn git_bin_path() -> Result<PathBuf, GitError> {
    GIT_BIN_PATH.clone()
}

pub(crate) fn git_version() -> Result<GitVersion, GitError> {
    GIT_VERSION.clone()
}

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
#[derive(Clone)]
pub struct GitCommand {
    cwd: PathBuf,
    git_bin: PathBuf,
    timeout: Duration,
    max_retries: u32,
    cancel: Option<CancellationToken>,
    git_version: GitVersion,
    env_vars: Vec<(String, String)>,
    circuit_breaker: Option<crate::circuit::CircuitBreaker>,
    progress_callback: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
}

impl std::fmt::Debug for GitCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitCommand")
            .field("cwd", &self.cwd)
            .field("git_bin", &self.git_bin)
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .field("cancel", &self.cancel)
            .field("git_version", &self.git_version)
            .field("env_vars", &self.env_vars)
            .field("circuit_breaker", &self.circuit_breaker.is_some())
            .field("progress_callback", &self.progress_callback.is_some())
            .finish()
    }
}

impl GitCommand {
    /// Create a new command runner for the given working directory.
    pub fn new(cwd: impl AsRef<Path>) -> Result<Self, GitError> {
        let cwd = cwd.as_ref().to_path_buf();
        let git_bin = GIT_BIN_PATH.clone()?;
        let git_version = git_version()?;
        crate::parse::check_git_version_compat(git_version);
        Ok(Self {
            cwd,
            git_bin,
            timeout: GIT_TIMEOUT,
            max_retries: MAX_RETRIES,
            cancel: None,
            git_version,
            env_vars: Vec::new(),
            circuit_breaker: None,
            progress_callback: None,
        })
    }

    pub(crate) fn git_bin(&self) -> &Path {
        &self.git_bin
    }

    pub(crate) fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub(crate) fn git_version(&self) -> GitVersion {
        self.git_version
    }

    #[cfg(test)]
    pub(crate) fn new_with_git_bin(cwd: PathBuf, git_bin: PathBuf) -> Self {
        // Tests use fake scripts; default to a known-tested version.
        let git_version = GitVersion {
            major: 2,
            minor: 45,
            patch: 1,
        };
        Self {
            cwd,
            git_bin,
            timeout: GIT_TIMEOUT,
            max_retries: MAX_RETRIES,
            cancel: None,
            git_version,
            env_vars: Vec::new(),
            circuit_breaker: None,
            progress_callback: None,
        }
    }

    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Add a custom environment variable to all subsequent commands.
    pub(crate) fn with_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Attach a circuit breaker for network operations.
    pub(crate) fn with_circuit_breaker(mut self, breaker: crate::circuit::CircuitBreaker) -> Self {
        self.circuit_breaker = Some(breaker);
        self
    }

    /// Attach a progress callback for network operations.
    ///
    /// The callback is invoked with each line of stderr as it is produced,
    /// which is useful for reporting clone/fetch/push progress to users.
    pub(crate) fn with_progress(
        mut self,
        callback: impl Fn(String) + Send + Sync + 'static,
    ) -> Self {
        self.progress_callback = Some(std::sync::Arc::new(callback));
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

        #[cfg(feature = "metrics")]
        let start = std::time::Instant::now();

        let result = self.run_inner(args, extra_env, &command_str).await;

        #[cfg(feature = "metrics")]
        {
            metrics::histogram!("gitr_command_duration_seconds", "command" => command_str.clone())
                .record(start.elapsed().as_secs_f64());
            match &result {
                Ok(out) if out.exit_code == 0 => {
                    metrics::counter!("gitr_commands_total", "command" => command_str.clone(), "status" => "success")
                        .increment(1);
                }
                _ => {
                    metrics::counter!("gitr_commands_total", "command" => command_str.clone(), "status" => "error")
                        .increment(1);
                }
            }
        }

        result
    }

    async fn run_inner(
        &self,
        args: &[impl AsRef<OsStr>],
        extra_env: &[(&str, &str)],
        command_str: &str,
    ) -> Result<CommandOutput, GitError> {
        let is_network = is_network_op(command_str);
        if is_network {
            if let Some(cb) = &self.circuit_breaker {
                if !cb.allow() {
                    return Err(GitError::NetworkError(format!(
                        "circuit breaker open for {command_str}"
                    )));
                }
            }
        }

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

            for (k, v) in &self.env_vars {
                cmd.env(k, v);
            }

            for (k, v) in extra_env {
                cmd.env(k, v);
            }

            let has_progress = is_network && self.progress_callback.is_some();

            let result: Result<Result<CommandOutput, std::io::Error>, GitError> = if has_progress {
                self.run_with_progress(&mut cmd, command_str).await
            } else if let Some(c) = &self.cancel {
                let output_fut = cmd.output();
                tokio::pin!(output_fut);
                tokio::select! {
                    biased;
                    _ = c.cancelled() => Err(GitError::Io("cancelled".to_string())),
                    r = tokio::time::timeout(self.timeout, output_fut) => match r {
                        Ok(Ok(o)) => Ok(Ok(CommandOutput {
                            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                            exit_code: o.status.code().unwrap_or(-1),
                        })),
                        Ok(Err(e)) => Ok(Err(e)),
                        Err(_) => Err(GitError::Timeout(self.timeout, command_str.to_string())),
                    },
                }
            } else {
                match tokio::time::timeout(self.timeout, cmd.output()).await {
                    Ok(Ok(o)) => Ok(Ok(CommandOutput {
                        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                        exit_code: o.status.code().unwrap_or(-1),
                    })),
                    Ok(Err(e)) => Ok(Err(e)),
                    Err(_) => Err(GitError::Timeout(self.timeout, command_str.to_string())),
                }
            };

            match result {
                Ok(Ok(output)) => {
                    let exit_code = output.exit_code;
                    if exit_code == 0 {
                        if is_network {
                            if let Some(cb) = &self.circuit_breaker {
                                cb.record_success();
                            }
                        }
                        return Ok(output);
                    }
                    if attempt < self.max_retries && is_retryable(&output.stderr) {
                        attempt += 1;
                        let backoff = Duration::from_millis(100 * 2_u64.pow(attempt - 1));
                        sleep(backoff).await;
                        continue;
                    }

                    if is_network {
                        if let Some(cb) = &self.circuit_breaker {
                            cb.record_failure();
                        }
                    }
                    return Err(GitError::CommandFailed {
                        command: command_str.to_string(),
                        exit_code,
                        stderr: output.stderr,
                        stdout: output.stdout,
                    });
                }
                Ok(Err(e)) => {
                    if is_network {
                        if let Some(cb) = &self.circuit_breaker {
                            cb.record_failure();
                        }
                    }
                    return Err(GitError::Io(format!("failed to run git: {e}")));
                }
                Err(GitError::Timeout(timeout, cmd)) => {
                    if is_network {
                        if let Some(cb) = &self.circuit_breaker {
                            cb.record_failure();
                        }
                    }
                    if attempt < self.max_retries {
                        attempt += 1;
                        let backoff = Duration::from_millis(100 * 2_u64.pow(attempt - 1));
                        sleep(backoff).await;
                        continue;
                    }
                    return Err(GitError::Timeout(timeout, cmd));
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn run_with_progress(
        &self,
        cmd: &mut Command,
        command_str: &str,
    ) -> Result<Result<CommandOutput, std::io::Error>, GitError> {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return Ok(Err(e)),
        };

        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| GitError::Io("failed to capture child stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| GitError::Io("failed to capture child stderr".to_string()))?;
        let cb = self.progress_callback.clone();

        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut stderr_buf = String::new();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if let Some(ref callback) = cb {
                            callback(line.clone());
                        }
                        stderr_buf.push_str(&line);
                        stderr_buf.push('\n');
                    }
                    Ok(None) => break,
                    Err(e) => {
                        // Stream error (e.g. broken pipe). Preserve what we have.
                        stderr_buf.push_str(&format!("<stderr read error: {e}>"));
                        break;
                    }
                }
            }
            stderr_buf
        });

        let stdout_handle = tokio::spawn(async move {
            let mut buf = Vec::new();
            // Safe to ignore: if stdout read fails, we just get empty stdout.
            #[allow(unused_must_use)]
            let _ = AsyncReadExt::read_to_end(&mut stdout, &mut buf).await;
            buf
        });

        let wait_result = if let Some(c) = &self.cancel {
            tokio::select! {
                biased;
                _ = c.cancelled() => {
                    // Best-effort kill; ignore errors since we are already cancelling.
                    #[allow(unused_must_use)]
                    let _ = child.kill().await;
                    return Err(GitError::Io("cancelled".to_string()));
                }
                r = tokio::time::timeout(self.timeout, child.wait()) => r,
            }
        } else {
            tokio::time::timeout(self.timeout, child.wait()).await
        };

        // If the spawned task panicked or was aborted, proceed without the stream.
        let stderr: String = stderr_handle.await.unwrap_or_default();
        let stdout_bytes: Vec<u8> = stdout_handle.await.unwrap_or_default();
        let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();

        match wait_result {
            Ok(Ok(status)) => Ok(Ok(CommandOutput {
                stdout,
                stderr,
                exit_code: status.code().unwrap_or(-1),
            })),
            Ok(Err(e)) => Ok(Err(e)),
            Err(_) => Err(GitError::Timeout(self.timeout, command_str.to_string())),
        }
    }
}

fn is_network_op(command_str: &str) -> bool {
    command_str.starts_with("clone ")
        || command_str.starts_with("fetch ")
        || command_str.starts_with("push ")
        || command_str.starts_with("pull ")
        || command_str.starts_with("ls-remote ")
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
        write_script(&script, "#!/bin/sh\nwhile true; do sleep 1; done\n");
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_timeout(Duration::from_millis(100));
        let err = cmd.run(&["status"]).await.unwrap_err();
        assert!(
            matches!(err, GitError::Timeout(_, _)),
            "expected Timeout, got {:?}",
            err
        );
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

    #[tokio::test]
    #[cfg(unix)]
    async fn test_progress_callback() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(
            &script,
            "#!/bin/sh\necho 'remote: counting objects' >&2\necho 'remote: compressing objects' >&2\necho done\n",
        );
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let lines2 = lines.clone();
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script).with_progress(
            move |line: String| {
                lines2.lock().unwrap().push(line);
            },
        );
        let out = cmd.run(&["fetch", "origin"]).await.unwrap();
        assert_eq!(out.stdout.trim(), "done");
        let collected = lines.lock().unwrap();
        assert!(collected.iter().any(|l| l.contains("counting objects")));
        assert!(collected.iter().any(|l| l.contains("compressing objects")));
    }

    #[test]
    fn test_git_command_debug() {
        let tmp = tempfile::tempdir().unwrap();
        let cmd = GitCommand::new(tmp.path()).unwrap();
        let s = format!("{:?}", cmd);
        assert!(s.contains("GitCommand"));
        assert!(s.contains("cwd"));
    }

    #[test]
    fn test_git_bin_path_and_version() {
        let path = git_bin_path().unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name == "git" || name == "git.exe");
        let version = git_version().unwrap();
        assert!(version.major >= 2);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_circuit_breaker_open() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(&script, "#!/bin/sh\necho 'error' >&2\nexit 1\n");
        let cb = crate::circuit::CircuitBreaker::new(1, 1, Duration::from_secs(60));
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_circuit_breaker(cb.clone());
        // First failure
        let _ = cmd.run(&["fetch", "origin"]).await;
        cb.record_failure();
        // Circuit breaker should be open now
        let err = cmd.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(matches!(err, GitError::NetworkError(ref s) if s.contains("circuit breaker open")));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_run_with_env() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(&script, "#!/bin/sh\necho $EXTRA_VAR\n");
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script);
        let out = cmd
            .run_with_env(&["status"], &[("EXTRA_VAR", "hello")])
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "hello");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cancel_in_run_with_progress() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(
            &script,
            "#!/bin/sh\nfor i in $(seq 1 100); do echo 'line' >&2; sleep 0.05; done\necho done\n",
        );
        let cancel = CancellationToken::new();
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_progress(|_line: String| {})
            .with_cancel(cancel.clone());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            cancel.cancel();
        });
        let err = cmd.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(
            matches!(err, GitError::Io(ref s) if s == "cancelled"),
            "expected cancelled, got {:?}",
            err
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_circuit_breaker_record_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(
            &script,
            "#!/bin/sh\necho 'fatal: unable to access: early EOF' >&2\nexit 1\n",
        );
        let cb = crate::circuit::CircuitBreaker::new(1, 1, Duration::from_secs(60));
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_circuit_breaker(cb.clone());
        let err = cmd.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(matches!(err, GitError::CommandFailed { .. }));
        // After failure, circuit breaker should have recorded failure
        cb.record_failure();
        assert!(!cb.allow());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_circuit_breaker_record_success() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(&script, "#!/bin/sh\necho ok\n");
        let cb = crate::circuit::CircuitBreaker::new(2, 1, Duration::from_secs(60));
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_circuit_breaker(cb.clone());
        let out = cmd.run(&["fetch", "origin"]).await.unwrap();
        assert_eq!(out.stdout.trim(), "ok");
        assert!(cb.allow());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_cancel_success() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(&script, "#!/bin/sh\necho done\n");
        let cancel = CancellationToken::new();
        let cmd =
            GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script).with_cancel(cancel);
        let out = cmd.run(&["status"]).await.unwrap();
        assert_eq!(out.stdout.trim(), "done");
    }

    #[tokio::test]
    async fn test_run_with_invalid_git() {
        let tmp = tempfile::tempdir().unwrap();
        let cmd = GitCommand::new_with_git_bin(
            tmp.path().to_path_buf(),
            PathBuf::from("/nonexistent/git"),
        );
        let err = cmd.run(&["status"]).await.unwrap_err();
        assert!(matches!(err, GitError::Io(ref s) if s.contains("failed to run git")));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_run_with_progress_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(&script, "#!/bin/sh\nsleep 2\necho done\n");
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_timeout(Duration::from_millis(100))
            .with_progress(|_line: String| {});
        let err = cmd.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(matches!(err, GitError::Timeout(_, _)));
    }

    #[tokio::test]
    async fn test_cancel_io_error() {
        let tmp = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        let cmd = GitCommand::new_with_git_bin(
            tmp.path().to_path_buf(),
            PathBuf::from("/nonexistent/git"),
        )
        .with_cancel(cancel);
        let err = cmd.run(&["status"]).await.unwrap_err();
        assert!(matches!(err, GitError::Io(ref s) if s.contains("failed to run git")));
    }

    #[tokio::test]
    async fn test_run_with_progress_spawn_error() {
        let tmp = tempfile::tempdir().unwrap();
        let cmd = GitCommand::new_with_git_bin(
            tmp.path().to_path_buf(),
            PathBuf::from("/nonexistent/git"),
        )
        .with_progress(|_line: String| {});
        let err = cmd.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(matches!(err, GitError::Io(ref s) if s.contains("failed to run git")));
    }

    #[tokio::test]
    async fn test_network_io_error_with_retry() {
        let tmp = tempfile::tempdir().unwrap();
        let cb = crate::circuit::CircuitBreaker::new(1, 1, Duration::from_secs(60));
        let cmd = GitCommand::new_with_git_bin(
            tmp.path().to_path_buf(),
            PathBuf::from("/nonexistent/git"),
        )
        .with_circuit_breaker(cb)
        .with_max_retries(1);
        let err = cmd.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(matches!(err, GitError::Io(ref s) if s.contains("failed to run git")));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_network_timeout_with_retry() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("git");
        write_script(&script, "#!/bin/sh\nsleep 2\n");
        let cb = crate::circuit::CircuitBreaker::new(1, 1, Duration::from_secs(60));
        let cmd = GitCommand::new_with_git_bin(tmp.path().to_path_buf(), script)
            .with_circuit_breaker(cb)
            .with_timeout(Duration::from_millis(100))
            .with_max_retries(1);
        let err = cmd.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(matches!(err, GitError::Timeout(_, _)));
    }
}
