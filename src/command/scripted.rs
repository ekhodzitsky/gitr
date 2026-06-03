use crate::command::CommandOutput;
use crate::error::GitError;
use std::collections::HashMap;

/// A hermetic git runner for tests that replays scripted responses.
///
/// Only available when the `test-utils` feature is enabled or when running tests.
///
/// Register expected commands with [`ScriptedRunner::script`], then use the
/// runner in place of `GitCommand` in tests.
///
/// # Example
///
/// ```
/// use gitr::{CommandOutput, ScriptedRunner};
/// use gitr::Error as GitError;
///
/// let mut runner = ScriptedRunner::new();
/// runner.script("status --porcelain", Ok(CommandOutput {
///     stdout: " M file.txt".to_string(),
///     stderr: String::new(),
///     exit_code: 0,
/// }));
///
/// // In an async context:
/// // let out = runner.run(&["status", "--porcelain"]).await.unwrap();
/// ```
#[derive(Debug, Clone, Default)]
pub struct ScriptedRunner {
    scripts: HashMap<String, Result<CommandOutput, GitError>>,
}

impl ScriptedRunner {
    /// Create a new empty scripted runner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a scripted response for a command.
    pub fn script(&mut self, command: impl Into<String>, result: Result<CommandOutput, GitError>) {
        self.scripts.insert(command.into(), result);
    }

    /// Run a command by looking up the scripted response.
    ///
    /// # Errors
    ///
    /// Returns `GitError::Io("unscripted command: ...")` if the command was not
    /// registered.
    #[allow(clippy::unused_async)]
    pub async fn run(&self, args: &[&str]) -> Result<CommandOutput, GitError> {
        let key = args.join(" ");
        self.lookup(&key)
    }

    /// Run with environment variables (same as `run`; env is ignored for lookup).
    #[allow(clippy::unused_async)]
    pub async fn run_with_env(
        &self,
        args: &[&str],
        _extra_env: &[(&str, &str)],
    ) -> Result<CommandOutput, GitError> {
        let key = args.join(" ");
        self.lookup(&key)
    }

    fn lookup(&self, key: &str) -> Result<CommandOutput, GitError> {
        match self.scripts.get(key) {
            Some(result) => result.clone(),
            None => Err(GitError::Io(format!("unscripted command: {key}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scripted_runner_ok() {
        let mut runner = ScriptedRunner::new();
        runner.script(
            "status --porcelain",
            Ok(CommandOutput {
                stdout: " M file.txt".to_string(),
                stderr: String::new(),
                exit_code: 0,
            }),
        );
        let out = runner.run(&["status", "--porcelain"]).await.unwrap();
        assert_eq!(out.stdout, " M file.txt");
    }

    #[tokio::test]
    async fn test_scripted_runner_err() {
        let mut runner = ScriptedRunner::new();
        runner.script(
            "push origin main",
            Err(GitError::CommandFailed {
                command: "push origin main".to_string(),
                exit_code: 1,
                stderr: "no remote".to_string(),
                stdout: String::new(),
            }),
        );
        let err = runner.run(&["push", "origin", "main"]).await.unwrap_err();
        assert!(matches!(err, GitError::CommandFailed { .. }));
    }

    #[tokio::test]
    async fn test_scripted_runner_unscripted() {
        let runner = ScriptedRunner::new();
        let err = runner.run(&["fetch", "origin"]).await.unwrap_err();
        assert!(matches!(err, GitError::Io(ref s) if s.contains("unscripted")));
    }
}
