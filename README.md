# `gitr`

[![CI](https://github.com/ekhodzitsky/gitr/actions/workflows/ci.yml/badge.svg)](https://github.com/ekhodzitsky/gitr/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/gitr.svg)](https://crates.io/crates/gitr)
[![docs.rs](https://docs.rs/gitr/badge.svg)](https://docs.rs/gitr)
[![Coverage](https://img.shields.io/badge/coverage-89.1%25-brightgreen)](https://github.com/ekhodzitsky/gitr)
[![MSRV](https://img.shields.io/badge/MSRV-1.80-orange)](https://github.com/ekhodzitsky/gitr/blob/main/Cargo.toml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Async typed git CLI wrapper for agents and automation.

## Why `gitr`?

| Approach | Async | Zero C deps | Worktrees | Rebase | Merge-tree |
|---|---|---|---|---|---|
| `git2` (libgit2) | ❌ needs `spawn_blocking` | ❌ | ✅ | ✅ | ❌ |
| `gix` (gitoxide) | ⚠️ partial | ✅ | ❌ | ❌ | ❌ |
| **`gitr`** | ✅ native | ✅ | ✅ | ✅ | ✅ |

`gitr` shells out to the `git` binary and provides structured types, typed errors,
and porcelain parsing. It is the only pure-Rust approach with **full feature coverage**
for AI agent workflows.

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

### Check status and commit

```rust
use gitr::Repository;

#[tokio::main]
async fn main() -> Result<(), gitr::Error> {
    let repo = Repository::open(".").await?;

    // Ensure clean working tree
    repo.ensure_clean().await?;

    // Stage, commit, push
    repo.add_all().await?;
    let sha = repo.commit("feat: agent work", &[]).await?;
    repo.push("origin", "main", false).await?;

    println!("Committed {sha}");
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

    wt.add_all().await?;
    wt.commit("feat: agent work in worktree", &[]).await?;

    repo.worktree_remove("/tmp/wt-1", false).await?;
    Ok(())
}
```

### Read-only merge conflict detection

```rust
use gitr::Repository;

#[tokio::main]
async fn main() -> Result<(), gitr::Error> {
    let repo = Repository::open(".").await?;

    let result = repo.merge_tree("main", "feature-x").await?;
    if result.has_conflicts {
        println!("Conflicts: {:?}", result.conflict_files);
    } else {
        println!("Clean merge");
    }
    Ok(())
}
```

## API overview

### `Repository`

- **Status:** `ensure_clean`, `status`, `changed_files`, `conflicted_files`, `untracked_files`
- **Branch:** `current_branch`, `branch_create`, `branch_delete`, `branch_exists`, `checkout`, `default_branch`
- **Commit & Push:** `commit`, `push`, `push_force`, `fetch`, `remote_url`
- **Worktree:** `worktree_add`, `worktree_remove`, `worktree_list`, `open_worktree`
- **Merge & Rebase:** `merge`, `merge_tree`, `rebase`, `rebase_continue`, `rebase_abort`
- **Stash:** `stash`, `stash_pop`
- **Diff:** `diff`, `diff_files`
- **Stage:** `add`, `add_all`

### Errors

`gitr::Error` (`GitError`) provides typed variants for every failure mode:

- `NotARepo` — path lacks `.git`
- `GitNotFound` — `git` binary not in `PATH`
- `CommandFailed` — non-zero exit with stdout/stderr captured
- `Timeout` — command exceeded time budget (default 60s)
- `Dirty` — working tree has changes
- `BranchExists` / `BranchNotFound`
- `WorktreeExists`
- `MergeConflicts`

## Feature flags

| Feature | Default | Description |
|---|---|---|
| `tracing` | ✅ | Emit `tracing` spans for command execution. |

## Testing

```bash
# All tests (unit + integration + doc-tests)
cargo test --all-features

# With coverage
cargo tarpaulin --all-features --fail-under 85

# Feature powerset
cargo hack check --feature-powerset --all-targets
```

## MSRV

Rust **1.80**.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

See [SECURITY.md](SECURITY.md).

## License

MIT
