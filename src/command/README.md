# `command`

Async git CLI executor. Wraps `tokio::process::Command` with timeouts and output capture.

## Purpose

This module is the **I/O edge** of `gitr`. It discovers the `git` binary via `which`,
spawns subprocesses with `tokio::process::Command`, and captures stdout/stderr/exit code.
All higher-level modules (`repo`, `parse`) build on top of this primitive.

## Public API

### Types

- `CommandOutput` — stdout, stderr, and exit code from a git invocation.
- `GitCommand` — low-level command runner bound to a working directory.

### Functions

- `GitCommand::new(cwd)` — discover `git` binary and bind to `cwd`.
- `GitCommand::run(args)` — run a command with the default 60-second timeout.
- `GitCommand::run_with_timeout(args, duration)` — run with a custom timeout.
- `GitCommand::run_with_env(args, envs)` — run with extra environment variables.

## Invariants

- The `git` binary path is resolved once at construction time, not per-call.
- All commands set `current_dir()` to the repository root.
- All commands set `kill_on_drop(true)` to prevent zombie processes.
- `run_with_timeout` must apply `tokio::time::timeout` (currently a known gap).

## Consumers

- `repo::Repository` — uses `GitCommand` for all git operations.
- Future `ScriptedRunner` / `RecordingRunner` — will implement the same interface for hermetic tests.
