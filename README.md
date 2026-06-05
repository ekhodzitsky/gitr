# `gitr`

<img src="assets/demo.gif" width="720" alt="gitr CLI demo">

[![CI](https://github.com/ekhodzitsky/gitr/actions/workflows/ci.yml/badge.svg)](https://github.com/ekhodzitsky/gitr/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/gitr.svg)](https://crates.io/crates/gitr)
[![docs.rs](https://docs.rs/gitr/badge.svg)](https://docs.rs/gitr)
[![Coverage](https://img.shields.io/badge/coverage-92%25-brightgreen)](https://github.com/ekhodzitsky/gitr)
[![MSRV](https://img.shields.io/badge/MSRV-1.80-orange)](https://github.com/ekhodzitsky/gitr/blob/main/Cargo.toml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Git for AI agents.** Async typed git CLI wrapper with JSON output and MCP server.

## Why `gitr`?

| Approach | Async | Zero C deps | Worktrees | Rebase | Merge-tree | CLI | MCP |
|---|---|---|---|---|---|---|---|
| `git2` (libgit2) | ❌ needs `spawn_blocking` | ❌ | ✅ | ✅ | ❌ | ❌ | ❌ |
| `gix` (gitoxide) | ⚠️ partial | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **`gitr`** | ✅ native | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

`gitr` shells out to the `git` binary and provides structured types, typed errors,
and porcelain parsing. It is the only pure-Rust approach with **full feature coverage**
for AI agent workflows — and it ships with a **CLI** and an **MCP server** out of the box.

---

## Installation

### Library

```toml
[dependencies]
gitr = "0.4"
```

### CLI

```bash
cargo install --git https://github.com/ekhodzitsky/gitr --bin gitr
```

### MCP Server

```bash
cargo install --git https://github.com/ekhodzitsky/gitr --bin gitr-mcp
```

---

## CLI Quick Start

```bash
# JSON status — perfect for scripts and agents
gitr status --json
# {"staged":[],"unstaged":["src/main.rs"],"untracked":[]}

# CI-friendly health check
gitr check
# {"clean":true,"conflicts":false,"untracked":false,"ok":true}

# Worktree switch in one shot
gitr worktree switch feature-x

# Diff with shortstat
gitr diff --stat
# 1 files changed, 10 insertions(+), 2 deletions(-)
```

---

## MCP Server Quick Start

`gitr-mcp` exposes git operations via Model Context Protocol (JSON-RPC over stdio).

### Claude Desktop / Cursor

Add to your MCP config (`claude_desktop_config.json` or `.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "gitr": {
      "command": "gitr-mcp",
      "env": {
        "GITR_REPO_PATH": "/path/to/your/repo"
      }
    }
  }
}
```

### Tools exposed

| Tool | Description |
|---|---|
| `git_status` | Structured working tree status |
| `git_check` | Clean / conflicts / untracked summary |
| `git_log` | Commit history |
| `git_log_stream` | Stream commit history (buffer-free) |
| `git_branch_current` | Current branch name |
| `git_checkout` | Switch branches |
| `git_commit` | Commit with message |
| `git_commit_signed` | GPG-signed commit |
| `git_verify_commit` | Verify commit GPG signature |
| `git_worktree_list` | List worktrees |
| `git_worktree_add` | Add a worktree |
| `git_submodule_list` | List submodules |
| `git_submodule_add` | Add a submodule |
| `git_submodule_update` | Update submodules |
| `git_config_get` | Read git config value |
| `git_config_set` | Write git config value |
| `git_tag_list` | List tags |
| `git_tag_create` | Create a tag |
| `git_stash_list` | List stash entries |
| `git_show` | Show file contents at a revision |
| `git_grep` | Search repository content |
| `git_ls_files` | List tracked / untracked / deleted files |
| `git_diff_cached` | Staged diff |
| `git_archive` | Create archive from a ref |
| `git_reset` | Reset index and working tree |
| `git_cherry_pick` | Cherry-pick commits |
| `git_describe` | Describe current commit |
| `git_clean` | Remove untracked files |
| `git_clone` | Clone a remote repository |
| `git_init` | Initialize a new repository |

---

## Library Quick Start

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

    repo.ensure_clean().await?;
    repo.add_all().await?;
    let sha = repo.commit("feat: agent work", &[], false).await?;
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
    wt.commit("feat: agent work in worktree", &[], false).await?;

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

### Structured diff

```rust
use gitr::Repository;

#[tokio::main]
async fn main() -> Result<(), gitr::Error> {
    let repo = Repository::open(".").await?;

    for file in repo.diff_structured().await? {
        println!("{:?} -> {:?}", file.old_path, file.new_path);
        for hunk in &file.hunks {
            for line in &hunk.lines {
                match line.kind {
                    gitr::DiffLineKind::Insertion => println!("+ {}", line.content),
                    gitr::DiffLineKind::Deletion => println!("- {}", line.content),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
```

---

## API overview

### `Repository`

- **Open / Init:** `open`, `init`, `clone`, `open_worktree`
- **Status:** `ensure_clean`, `status`, `status_z`, `changed_files`, `conflicted_files`, `untracked_files`, `is_nothing_to_commit`, `has_untracked_files`, `is_merge_conflict`
- **Branch:** `current_branch`, `branch_create`, `branch_delete`, `branch_exists`, `checkout`, `default_branch`
- **Commit & Push:** `commit`, `commit_signed`, `push`, `push_force`, `fetch`, `remote_url`
- **Worktree:** `worktree_add`, `worktree_remove`, `worktree_list`
- **Merge & Rebase:** `merge`, `merge_tree`, `rebase`, `rebase_continue`, `rebase_abort`
- **Stash:** `stash`, `stash_pop`, `stash_list`
- **Diff:** `diff`, `diff_cached`, `diff_files`, `diff_shortstat`, `diff_structured`, `diff_cached_structured`, `diff_files_structured`
- **Stage:** `add`, `add_all`
- **Config:** `config_get`, `config_set`
- **Tag:** `tag_list`, `tag_create`
- **Submodule:** `submodule_list`, `submodule_add`, `submodule_update`, `submodule_deinit`, `submodule_sync`
- **Query:** `show`, `blame`, `blame_structured`, `blame_stream` (with `stream` feature), `grep`, `grep_stream` (with `stream` feature), `ls_files`, `ls_files_stream` (with `stream` feature), `log`, `log_paginated`, `log_stream` (with `stream` feature), `describe`, `clean`, `format_patch`, `apply_patch`, `apply_patch_file`, `reflog_list`, `reflog_expire`, `hooks_list`, `hook_install`, `hook_remove`, `run_hook`, `run_hook_with_timeout`, `run_hook_streaming`, `bisect_start`, `bisect_bad`, `bisect_good`, `bisect_reset`, `bisect_run`, `notes_list`, `notes_show`, `notes_add`, `notes_remove`, `check_ignore`, `check_attr`, `bundle_create`, `bundle_list_heads`, `bundle_verify`, `bundle_unbundle`
- **Object DB:** `hash_object`, `write_blob`, `mktree`, `write_tree`, `read_object`, `read_tree`, `read_blob`, `read_commit`, `write_commit`
- **Index:** `read_index`, `update_index`, `remove_index`
- **LFS:** `lfs_track`, `lfs_untrack`, `lfs_ls_files`, `lfs_lock`, `lfs_unlock`
- **Sparse Checkout:** `sparse_checkout_init`, `sparse_checkout_set`, `sparse_checkout_add`, `sparse_checkout_disable`, `sparse_checkout_list`
- **Reset:** `reset`, `cherry_pick`
- **Helpers:** `with_cache`, `with_cancel`, `with_timeout`, `invalidate_cache`

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
| `serde` | ❌ | Derive `Serialize`/`Deserialize` on public types (for JSON/MCP). |
| `stream` | ❌ | Enable streaming APIs (`log_stream`, `grep_stream`, `blame_stream`, `ls_files_stream`) returning `impl Stream`. |
| `metrics` | ❌ | Emit `metrics` counters and histograms for command execution. |
| `test-utils` | ❌ | Expose `ScriptedRunner` for downstream hermetic testing. |

## Builder Pattern

Complex commands expose `_opts` variants for extensibility while keeping the original API backwards-compatible:

```rust
use gitr::{Repository, PushOptions, CommitOptions};

let repo = Repository::open(".").await?;

// Original API still works
repo.push("origin", "main", false).await?;

// Builder variant for advanced options
repo.push_opts(&PushOptions {
    remote: "origin",
    branch: "main",
    force: false,
    force_with_lease: true,
    set_upstream: true,
}).await?;

repo.commit_opts(&CommitOptions::new("fix typo")
    .amend(true)
    .no_verify(true)
).await?;
```

## Testing

`gitr` maintains a comprehensive test matrix:

- **Unit tests** — parser correctness with real git output fixtures.
- **Snapshot tests** (`insta`) — 41 snapshots covering all porcelain parsers across git versions.
- **Property-based tests** (`proptest`) — roundtrip and never-panics properties for `parse_status`, `parse_diff_shortstat`, `parse_log_line`, `parse_grep`, and more.
- **Integration tests** — real git repository operations via `tempfile`.
- **Hermetic tests** — `ScriptedRunner` for recorded I/O (enabled via `test-utils` feature).

```bash
# All workspace tests (unit + integration + doc-tests)
cargo test --workspace --all-features

# Accept new insta snapshots
INSTA_UPDATE=always cargo test --workspace --all-features

# With coverage
cargo tarpaulin --workspace --all-features --fail-under 85

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
