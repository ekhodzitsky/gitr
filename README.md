# `gitr`

[![CI](https://github.com/ekhodzitsky/gitr/actions/workflows/ci.yml/badge.svg)](https://github.com/ekhodzitsky/gitr/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/gitr.svg)](https://crates.io/crates/gitr)
[![docs.rs](https://docs.rs/gitr/badge.svg)](https://docs.rs/gitr)
[![MSRV](https://img.shields.io/badge/MSRV-1.80-orange)](https://github.com/ekhodzitsky/gitr/blob/main/Cargo.toml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Async typed git CLI wrapper for agents and automation.

## Overview

`gitr` shells out to the `git` binary and provides:

* **Async-first** — every operation is `async` via `tokio::process::Command`.
* **Typed porcelain parsing** — `status`, `worktree list`, `log`, `branch` parsed into structs.
* **Structured errors** — distinguish dirty trees, merge conflicts, missing branches, timeouts.
* **Agent-centric API** — worktrees, rebase, merge-tree, stash: everything an AI agent needs.
* **Zero C dependencies** — pure Rust, fast compile, easy cross-compile.

## Quick start

Add to your `Cargo.toml`:

```toml
[dependencies]
gitr = "0.1"
```

### Open a repository

```rust
use gitr::Repository;

#[tokio::main]
async fn main() -> Result<(), gitr::Error> {
    let repo = Repository::open(".").await?;
    let branch = repo.current_branch().await?;
    println!("On branch: {branch}");
    Ok(())
}
```

### Worktree workflow

```rust
use gitr::Repository;

#[tokio::main]
async fn main() -> Result<(), gitr::Error> {
    let repo = Repository::open(".").await?;

    repo.worktree_add("/tmp/wt-1", "feature-x").await?;
    let wt = repo.open_worktree("/tmp/wt-1").await?;

    wt.checkout("feature-x").await?;
    // agent works here …
    wt.add_all().await?;
    wt.commit("feat: agent work", &[]).await?;

    repo.worktree_remove("/tmp/wt-1", false).await?;
    Ok(())
}
```

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `tracing` | ✅ | Emit `tracing` spans for command execution. |

## MSRV

Rust **1.80**.

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

MIT
