# gitr Workspace Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate `gitr` from a single crate into a Rust workspace with `crates/gitr` (lib), `crates/gitr-cli` (bin), and `crates/gitr-mcp` (bin), add `serde` support, and update CI.

**Architecture:** Workspace manifest in root; core lib moved to `crates/gitr/` unchanged except for optional `serde` feature; two new binary crates depend on `gitr` with `--features serde`; CI updated with `--workspace` flags.

**Tech Stack:** Rust 1.80, Tokio, clap, serde, serde_json, anyhow

---

## File Structure

```
gitr/
├── Cargo.toml                 # workspace manifest (NEW)
├── Cargo.lock                 # workspace-level (RETAIN)
├── crates/
│   ├── gitr/
│   │   ├── Cargo.toml         # moved from root (MODIFY)
│   │   ├── src/               # moved from root
│   │   └── tests/             # moved from root
│   ├── gitr-cli/
│   │   ├── Cargo.toml         # (NEW)
│   │   └── src/
│   │       └── main.rs        # (NEW)
│   └── gitr-mcp/
│       ├── Cargo.toml         # (NEW)
│       └── src/
│           └── main.rs        # (NEW)
├── .github/workflows/ci.yml   # (MODIFY)
├── deny.toml                  # (MODIFY)
└── docs/superpowers/specs/2026-06-03-workspace-design.md
```

---

### Task 1: Move library crate into workspace

**Files:**
- Create: `crates/gitr/Cargo.toml`
- Create: directory `crates/gitr/src/`, `crates/gitr/tests/`
- Move: `src/` → `crates/gitr/src/`
- Move: `tests/` → `crates/girr/tests/`
- Modify: `Cargo.toml` (root) → workspace manifest
- Delete: old root `src/`, `tests/` after move

- [ ] **Step 1: Create workspace manifest (`Cargo.toml`)**

```toml
[workspace]
members = ["crates/gitr", "crates/gitr-cli", "crates/gitr-mcp"]
resolver = "2"

[workspace.package]
version = "0.4.0"
edition = "2021"
rust-version = "1.80"
license = "MIT"
repository = "https://github.com/ekhodzitsky/gitr"
authors = ["Evgeny Khodzitsky"]

[workspace.dependencies]
tokio = { version = "1.44", features = ["rt-multi-thread", "macros", "process", "time"] }
gitr = { path = "crates/gitr" }
```

- [ ] **Step 2: Move existing source and tests**

Run:
```bash
mkdir -p crates/gitr
git mv src crates/gitr/src
git mv tests crates/gitr/tests
git mv Cargo.toml crates/gitr/Cargo.toml
```

- [ ] **Step 3: Update `crates/gitr/Cargo.toml`**

Replace the first lines to use workspace inheritance:

```toml
[package]
name = "gitr"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "Async typed git CLI wrapper for agents and automation."
keywords = ["git", "async", "cli", "agent", "worktree"]
categories = ["development-tools", "asynchronous"]
exclude = [".git", "target"]

[features]
default = ["tracing"]
tracing = ["dep:tracing"]
serde = ["dep:serde"]
test-utils = []

[dependencies]
tokio = { workspace = true, features = ["fs", "io-util", "macros", "process", "rt", "sync", "time"] }
thiserror = "2.0"
tracing = { version = "0.1", optional = true }
serde = { version = "1.0", features = ["derive"], optional = true }
which = "7.0"
async-trait = "0.1"

[dev-dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "test-util"] }
tempfile = "3.27"
tracing-subscriber = "0.3"
```

- [ ] **Step 4: Verify lib crate builds**

Run:
```bash
cd crates/gitr && cargo test --all-features
```

Expected: all 79 tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move gitr lib into workspace crates/gitr"
```

---

### Task 2: Add optional `serde` feature to `gitr` lib

**Files:**
- Modify: `crates/gitr/src/types/mod.rs`
- Modify: `crates/gitr/src/parse/mod.rs` (DiffShortstat)
- Modify: `crates/gitr/src/command/mod.rs` (CommandOutput)

- [ ] **Step 1: Add serde derives to `GitStatus`**

In `crates/gitr/src/types/mod.rs`, replace:

```rust
use std::path::PathBuf;
```

With:

```rust
use std::path::PathBuf;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
```

And add derive macros:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitStatus {
```

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitLogEntry {
```

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitRemote {
```

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitMergeResult {
```

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitWorktree {
```

- [ ] **Step 2: Add serde derive to `DiffShortstat`**

In `crates/gitr/src/parse/mod.rs`, at the top add:

```rust
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
```

And on the struct:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiffShortstat {
```

- [ ] **Step 3: Add serde derive to `CommandOutput`**

In `crates/gitr/src/command/mod.rs`, at the top add:

```rust
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
```

And on the struct:

```rust
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CommandOutput {
```

- [ ] **Step 4: Verify serde feature compiles**

Run:
```bash
cargo check -p gitr --features serde
```

Expected: clean compile, zero warnings.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: add optional serde feature for JSON serialization"
```

---

### Task 3: Update CI for workspace

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `deny.toml`

- [ ] **Step 1: Update `ci.yml` commands**

In `.github/workflows/ci.yml`, replace every `cargo test`, `cargo clippy`, `cargo doc` with `--workspace`:

```yaml
      - run: cargo test --workspace --all-features --locked
      - run: cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
      - run: cargo doc --workspace --no-deps --all-features --locked
```

Also update `cargo publish --dry-run`:

```yaml
      - run: cargo publish -p gitr --dry-run --all-features --locked
```

- [ ] **Step 2: Verify deny.toml works for workspace**

Run locally:
```bash
cargo deny check
```

Expected: passes (workspace-level graph is sufficient).

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "ci: update pipeline for workspace structure"
```

---

### Task 4: Create `gitr-cli` crate

**Files:**
- Create: `crates/gitr-cli/Cargo.toml`
- Create: `crates/gitr-cli/src/main.rs`

- [ ] **Step 1: Create `crates/gitr-cli/Cargo.toml`**

```toml
[package]
name = "gitr-cli"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "CLI for gitr — async typed git operations"
publish = false

[dependencies]
gitr = { workspace = true, features = ["serde", "tracing"] }
tokio = { workspace = true }
clap = { version = "4.5", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
```

- [ ] **Step 2: Create `crates/gitr-cli/src/main.rs`**

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use gitr::Repository;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gitr")]
#[command(about = "Async typed git CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Repository path (defaults to current directory)
    #[arg(short, long, global = true, default_value = ".")]
    path: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Show working tree status
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// CI-friendly repository health check
    Check,
    /// Show commit log
    Log {
        /// Limit number of commits
        #[arg(short = 'n', long)]
        max_count: Option<usize>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Worktree operations
    Worktree {
        #[command(subcommand)]
        cmd: WorktreeCommands,
    },
    /// Show unstaged diff
    Diff {
        /// Show shortstat instead of raw diff
        #[arg(long)]
        stat: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum WorktreeCommands {
    /// List worktrees
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create or switch to a worktree
    Switch {
        branch: String,
        /// Worktree path (defaults to ./<branch>)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo = Repository::open(&cli.path).await?;

    match cli.command {
        Commands::Status { json } => {
            let status = repo.status().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                for f in &status.staged {
                    println!("staged: {f}");
                }
                for f in &status.unstaged {
                    println!("unstaged: {f}");
                }
                for f in &status.untracked {
                    println!("untracked: {f}");
                }
            }
        }
        Commands::Check => {
            let clean = repo.ensure_clean().await.is_ok();
            let conflicts = repo.is_merge_conflict().await?;
            let untracked = repo.has_untracked_files().await?;
            let ok = clean && !conflicts && !untracked;
            let result = serde_json::json!({
                "clean": clean,
                "conflicts": conflicts,
                "untracked": untracked,
                "ok": ok,
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
            if !ok {
                std::process::exit(1);
            }
        }
        Commands::Log { max_count, json } => {
            let log = repo.log(max_count).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&log)?);
            } else {
                for entry in log {
                    println!("{} {} {}", &entry.short_sha, &entry.timestamp, &entry.message);
                }
            }
        }
        Commands::Worktree { cmd } => match cmd {
            WorktreeCommands::List { json } => {
                let list = repo.worktree_list().await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    for wt in list {
                        println!("{} -> {}", wt.path.display(), wt.branch);
                    }
                }
            }
            WorktreeCommands::Switch { branch, path } => {
                let path = path.unwrap_or_else(|| PathBuf::from(&branch));
                if !repo.branch_exists(&branch).await? {
                    repo.branch_create(&branch, None).await?;
                }
                repo.worktree_add(&path, &branch).await?;
                println!("worktree {} -> {}", path.display(), branch);
            }
        },
        Commands::Diff { stat, json } => {
            if stat {
                let s = repo.diff_shortstat().await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&s)?);
                } else {
                    println!(
                        "{} files changed, {} insertions(+), {} deletions(-)",
                        s.files_changed, s.insertions, s.deletions
                    );
                }
            } else {
                let d = repo.diff().await?;
                println!("{d}");
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Verify CLI builds**

Run:
```bash
cargo build -p gitr-cli
```

Expected: compiles successfully.

- [ ] **Step 4: Quick smoke test**

Run:
```bash
cargo run -p gitr-cli -- status --json
```

Expected: JSON status of current repo (or error if not a git repo).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(gitr-cli): add CLI with status, check, log, worktree, diff"
```

---

### Task 5: Create `gitr-mcp` crate

**Files:**
- Create: `crates/gitr-mcp/Cargo.toml`
- Create: `crates/gitr-mcp/src/main.rs`

- [ ] **Step 1: Create `crates/gitr-mcp/Cargo.toml`**

```toml
[package]
name = "gitr-mcp"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "MCP server for gitr — exposes git operations to AI agents"
publish = false

[dependencies]
gitr = { workspace = true, features = ["serde", "tracing"] }
tokio = { workspace = true, features = ["io-std", "io-util"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

- [ ] **Step 2: Create `crates/gitr-mcp/src/main.rs`**

```rust
use gitr::Repository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[tokio::main]
async fn main() {
    let repo_path = std::env::var("GITR_REPO_PATH").unwrap_or_else(|_| ".".to_string());
    let repo = match Repository::open(&repo_path).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to open repo: {e}");
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                write_response(
                    &mut stdout,
                    JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("parse error: {e}"),
                            data: None,
                        }),
                    },
                );
                continue;
            }
        };

        let result = handle_request(&repo, &req.method, req.params.clone()).await;
        let resp = match result {
            Ok(val) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: Some(val),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32000,
                    message: e,
                    data: None,
                }),
            },
        };

        write_response(&mut stdout, resp);
    }
}

fn write_response(stdout: &mut io::Stdout, resp: JsonRpcResponse) {
    let json = serde_json::to_string(&resp).unwrap();
    writeln!(stdout, "{json}").unwrap();
    stdout.flush().unwrap();
}

async fn handle_request(repo: &Repository, method: &str, params: Value) -> Result<Value, String> {
    match method {
        "git_status" => {
            let status = repo.status().await.map_err(|e| e.to_string())?;
            serde_json::to_value(status).map_err(|e| e.to_string())
        }
        "git_check" => {
            let clean = repo.ensure_clean().await.is_ok();
            let conflicts = repo.is_merge_conflict().await.map_err(|e| e.to_string())?;
            let untracked = repo.has_untracked_files().await.map_err(|e| e.to_string())?;
            let val = serde_json::json!({
                "clean": clean,
                "conflicts": conflicts,
                "untracked": untracked,
            });
            Ok(val)
        }
        "git_log" => {
            let max_count = params.get("max_count").and_then(|v| v.as_u64()).map(|v| v as usize);
            let log = repo.log(max_count).await.map_err(|e| e.to_string())?;
            serde_json::to_value(log).map_err(|e| e.to_string())
        }
        "git_branch_current" => {
            let branch = repo.current_branch().await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "branch": branch }))
        }
        "git_checkout" => {
            let branch = params
                .get("branch")
                .and_then(|v| v.as_str())
                .ok_or("missing branch")?;
            repo.checkout(branch).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_commit" => {
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or("missing message")?;
            let sha = repo.commit(message, &[] as &[&std::path::Path]).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "sha": sha }))
        }
        "git_worktree_list" => {
            let list = repo.worktree_list().await.map_err(|e| e.to_string())?;
            serde_json::to_value(list).map_err(|e| e.to_string())
        }
        "git_worktree_add" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            let branch = params
                .get("branch")
                .and_then(|v| v.as_str())
                .ok_or("missing branch")?;
            let wt = repo.worktree_add(&path, branch).await.map_err(|e| e.to_string())?;
            serde_json::to_value(wt).map_err(|e| e.to_string())
        }
        _ => Err(format!("unknown method: {method}")),
    }
}
```

- [ ] **Step 3: Verify MCP builds**

Run:
```bash
cargo build -p gitr-mcp
```

Expected: compiles successfully.

- [ ] **Step 4: Smoke test MCP**

Run:
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"git_status","params":{}}' | cargo run -p gitr-mcp
```

Expected: one line of JSON with `result` containing staged/unstaged/untracked arrays.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(gitr-mcp): add MCP server over stdio"
```

---

### Task 6: Final verification

- [ ] **Step 1: Run full workspace test suite**

```bash
cargo test --workspace --all-features
```

Expected: all tests pass (79+ existing; CLI and MCP have no tests yet).

- [ ] **Step 2: Run clippy**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: clean, zero warnings.

- [ ] **Step 3: Run doc check**

```bash
cargo doc --workspace --no-deps --all-features
```

Expected: builds without warnings.

- [ ] **Step 4: Run fmt check**

```bash
cargo fmt --check
```

Expected: passes.

- [ ] **Step 5: Run cargo-deny**

```bash
cargo deny check
```

Expected: passes.

- [ ] **Step 6: Run semver-checks**

```bash
cargo semver-checks -p gitr
```

Expected: passes (no breaking changes in lib API).

- [ ] **Step 7: Final commit**

```bash
git add -A
git commit -m "chore: verify workspace build and CI compliance"
```

---

## Self-Review Checklist

- [x] **Spec coverage:** Every section of `2026-06-03-workspace-design.md` is addressed by at least one task.
- [x] **Placeholder scan:** No TBD, TODO, or vague instructions. Every step has exact code/commands.
- [x] **Type consistency:** `serde_json::to_string_pretty`, `serde_json::json!`, `serde_json::to_value` used consistently across CLI and MCP.
- [x] **CI coverage:** Updated commands match spec (`--workspace`).
- [x] **Publish safety:** `gitr-cli` and `gitr-mcp` marked `publish = false`.
