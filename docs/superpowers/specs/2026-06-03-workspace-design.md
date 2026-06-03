# gitr Workspace Design — Lib + CLI + MCP

## Goal

Transform `gitr` from a single library crate into a workspace with three crates:
- `gitr` — core async typed git library (migrated from current root)
- `gitr-cli` — command-line tool for humans and automation
- `gitr-mcp` — Model Context Protocol server for AI agents

## Architecture

### Workspace Structure

```
gitr/
├── Cargo.toml                 # workspace manifest
├── crates/
│   ├── gitr/                  # core library
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── gitr-cli/              # CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs
│   └── gitr-mcp/              # MCP server binary
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
└── .github/workflows/ci.yml
```

### Crate Boundaries

| Crate | API | Key Dependencies | Publish |
|---|---|---|---|
| `gitr` | `Repository`, `GitApi`, `GitError`, parsers | `tokio`, `thiserror`, `which`, `async-trait`, `tracing` (opt), `serde` (opt) | ✅ |
| `gitr-cli` | binary only | `gitr`, `clap`, `serde_json`, `tokio`, `anyhow` | ❌ (`publish = false`) |
| `gitr-mcp` | binary only | `gitr`, `serde`, `serde_json`, `tokio` | ❌ (`publish = false`) |

### Decisions

**1. CLI commands (MVP)**
- `gitr status [--json]` — structured working tree status
- `gitr check` — CI-friendly aggregate check (clean, no conflicts, no untracked); exits 0/1
- `gitr log [--json] [-n N]` — commit history
- `gitr worktree list [--json]` — list worktrees
- `gitr worktree switch <branch>` — create or switch worktree in one shot
- `gitr diff [--stat] [--json]` — unstaged diff with optional shortstat

**2. MCP transport**
- `stdio` (JSON-RPC lines over stdin/stdout). This is the MCP standard for desktop integrations (Cursor, Claude Desktop, Windsurf). No HTTP server or SSE needed for MVP.

**3. serde support in `gitr` lib**
- Optional feature `serde` on `gitr` crate. Adds `Serialize`/`Deserialize` to all public data types (`GitStatus`, `GitMergeResult`, `GitWorktree`, `GitLogEntry`, `GitRemote`, `DiffShortstat`). CLI and MCP depend on `gitr` with `--features serde`.
- Rationale: avoids DTO duplication in CLI/MCP; downstream libraries get serde for free.

**4. Error handling in binaries**
- `gitr-cli` uses `anyhow` for ergonomic error reporting and colored output.
- `gitr-mcp` uses structured JSON-RPC error responses; no human-oriented formatting.

## MCP Protocol

### Tools exposed

| Tool | Input | Output |
|---|---|---|
| `git_status` | `{}` | `{ staged, unstaged, untracked }` |
| `git_check` | `{}` | `{ clean: bool, conflicts: bool, untracked: bool }` |
| `git_log` | `{ max_count?: number }` | `{ entries: [{ sha, message, author, timestamp }] }` |
| `git_branch_current` | `{}` | `{ branch: string }` |
| `git_checkout` | `{ branch: string }` | `{}` |
| `git_commit` | `{ message: string }` | `{ sha: string }` |
| `git_worktree_list` | `{}` | `{ worktrees: [{ path, branch }] }` |
| `git_worktree_add` | `{ path: string, branch: string }` | `{ path, branch }` |

### Transport

- Each line on stdin is a JSON-RPC 2.0 request.
- Each line on stdout is a JSON-RPC 2.0 response.
- Server lifecycle tied to parent process (no daemon mode for MVP).

## CI/CD Changes

- Replace `cargo test --all-features` with `cargo test --workspace --all-features`.
- Replace `cargo clippy --all-targets --all-features` with `cargo clippy --workspace --all-targets --all-features`.
- Replace `cargo doc --no-deps --all-features` with `cargo doc --workspace --no-deps --all-features`.
- `cargo deny check` — workspace-level.
- `cargo semver-checks` — check only `crates/gitr`; others have `publish = false`.
- `cargo publish --dry-run` — dry-run `crates/gitr` only.

## Migration Plan

### Phase 1 — Workspace skeleton
1. Create workspace `Cargo.toml`.
2. Move current `src/`, `Cargo.toml`, `tests/` into `crates/gitr/`.
3. Update all paths in CI.
4. Verify `cargo test --workspace` passes.

### Phase 2 — `gitr` enhancements
1. Add optional `serde` feature to `crates/gitr/Cargo.toml`.
2. Derive `Serialize`/`Deserialize` on public types behind `#[cfg(feature = "serde")]`.
3. Add `RecordingRunner` (test-utils enhancement).

### Phase 3 — `gitr-cli`
1. Create `crates/gitr-cli/` with `clap`, `anyhow`, `serde_json`.
2. Implement MVP commands: `status`, `check`, `log`, `worktree`, `diff`.
3. `cargo install --path crates/gitr-cli` works.

### Phase 4 — `gitr-mcp`
1. Create `crates/gitr-mcp/` with `serde`, `serde_json`.
2. Implement stdio JSON-RPC loop.
3. Implement tools table above.

## Future Work (post-MVP)

- `gitr commit --suggest` — generate commit message from diff via LLM.
- SSE transport for MCP (for web-based agents).
- `gitr clone` and `gitr init` commands.
- Conflict classification in `GitMergeResult`.

## Open Questions (resolved)

- **CLI scope:** MVP = 6 commands (see Decision 1).
- **MCP transport:** stdio (see Decision 2).
- **serde location:** optional feature in `gitr` lib (see Decision 3).
