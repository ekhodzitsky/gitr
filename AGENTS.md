# gitr Agent Guide

This file contains agent-level conventions for the `gitr` crate — an async typed
git CLI wrapper optimized for AI agents and automation.

## Contents

- [Meta Principle](#meta-principle)
- [Behavioral Guidelines](#behavioral-guidelines)
- [Library Contract Rules](#library-contract-rules-hard-constraints)
- [Rust Safety Rules](#rust-safety-rules-hard-constraints)
- [Error Handling Doctrine](#error-handling-doctrine-hard-constraints)
- [Git CLI Compatibility](#git-cli-compatibility-hard-constraints)
- [Build & Test](#build--test)
- [Clippy Lint Policy](#clippy-lint-policy)
- [Editing Rules](#editing-rules)

## Meta Principle

Before applying any rule or refactor, ask: **what problem does this solve?**
A newtype, a refactor, or an abstraction is justified only if it prevents a concrete bug,
clarifies an invariant, or removes a footgun. If the answer is "it looks better" — revert.
Decoration is not engineering.

## Behavioral Guidelines

- **State assumptions explicitly.** If uncertain, ask before implementing.
- **Minimum code.** No speculative abstractions. No features beyond the request.
  If you write 200 lines and it could be 50, rewrite.
- **Surgical changes only.** Touch only what you must. Match existing style.
  Remove imports/variables made unused by *your* changes; don't delete pre-existing
  dead code unless asked.
- **Goal-driven execution.** Every task needs verifiable success criteria and a brief plan
  (`Step → verify: check`).
- **Prefer `?` over `unwrap`/`expect` even in tests** where it keeps the test readable.

## Library Contract Rules (Hard Constraints)

These rules protect the crate's most fragile contracts: the public API surface,
git CLI compatibility, and porcelain parsing correctness.

1. **Public API is opt-in.** Prefer `pub(crate)`. New `pub` items require a concrete
   external caller or a paved-path rationale, plus a test.
2. **Git CLI facts must not go stale.** If command names, flags, porcelain formats,
   or error message patterns change, update `README.md`, `CHANGELOG.md`, and tests
   in the same change set.
3. **Porcelain parsing is a hard contract.** Any change to `parse_status`,
   `parse_worktrees`, `parse_branches`, or `parse_merge_tree` must include unit tests
   with sample output from real `git` invocations.
4. **Dependencies are architecture changes.** No new crate without a rationale:
   why std/local code is not enough, transitive impact, MSRV, license, and feature-flag
   consequences. Small helper crates are rejected by default.
5. **Refactors isolate mechanics from behavior.** File moves, splits, renames, and
   formatting-only changes must be separate from semantic changes whenever practical.
6. **MSRV is 1.80.** Do not use language or std features introduced after Rust 1.80.

## Rust Safety Rules (Hard Constraints)

These rules apply to **new or modified production code** under `src/` (outside
`#[cfg(test)]`). Violations in touched code must be fixed before merge.

1. **`unwrap()` is banned.** Use `?`, `if let`, `match`, `ok_or`, `bail!`, or `.context()`.
2. **`expect()` is banned.** No "this should never happen" — it always happens eventually.
3. **`panic!()` is banned.** Graceful degradation only; propagate errors via `Result`.
4. **`std::thread::sleep` is banned in `async fn`.** Use `tokio::time::sleep(...).await`.
5. **`std::sync::Mutex` is banned in `async fn`.** Use `tokio::sync::Mutex` to avoid blocking the executor.
6. **All external `Command::output().await` must have a `tokio::time::timeout`.** Prevent infinite hangs from rogue child processes.
7. **All `spawn()` calls must set `kill_on_drop(true)` or attach to a `CancellationToken`.** Prevent zombie processes.

### Preconditions & Invariants

Prefer expressing preconditions in types before comments or runtime checks:

1. Use a specific type/newtype/parser constructor that makes invalid states unrepresentable.
2. Keep fields private when they carry invariants.
3. If the invariant cannot be encoded in the type, document it and add a `debug_assert!` next to the use.

Example:

```rust
/// Average of a non-empty slice.
/// Precondition: `!items.is_empty()`
pub fn average(items: &[f64]) -> f64 {
    debug_assert!(!items.is_empty(), "precondition: non-empty slice");
    items.iter().sum::<f64>() / items.len() as f64
}
```

`debug_assert!` is a last-line development check, not a substitute for type design.

### Tests (`#[cfg(test)]`)

`unwrap()`/`expect()` are allowed for brevity, but prefer `?` where it keeps the test readable.

## Error Handling Doctrine (Hard Constraints)

### Meta Principle

**Every error has an owner, a representation, and a consumer. Never swallow an error.**

### 1. Typed Errors Only

- **Library code uses `thiserror`.** Every public function that can fail returns a specific
  `GitError` enum variant. Callers must be able to `match` on variants.
- **`anyhow` is banned from the public API.** If a function is `pub` and returns `anyhow::Result`,
  it must be refactored to a typed `thiserror` enum.
- **`thiserror` messages must be lowercase without trailing punctuation.**
  - Good: `#[error("branch not found: {0}")]`
  - Bad: `#[error("Branch not found: {0}.")]`

### 2. Error Context & Chaining

- **Preserve the cause chain.** `#[source]` or `#[from]` must be used so that the full
  chain is available to callers.
- **Add context at boundaries.** Git CLI spawn and parse boundaries should include
  enough detail to be actionable (e.g. the raw command and arguments).

### 3. Silent Errors Are Banned

- **`let _ = ...` on `Result` is banned unless explicitly justified.**
- **Explicit ignore requires a comment.**
  ```rust
  // Safe to ignore: receiver is gone because the transport closed.
  #[allow(unused_must_use)]
  let _ = sender.try_send(event);
  ```

## Git CLI Compatibility (Hard Constraints)

This crate shells out to the `git` binary. Compatibility with git CLI behavior is not optional.

### 1. Command Isolation

- Every git command invocation must specify the working directory via `current_dir()`.
- Environment variables must be explicitly set per-invocation (e.g. `GIT_EDITOR=true` for rebase).
- The `git` binary must be discovered via `PATH` search at initialization time, not per-call.

### 2. Porcelain Format Stability

- Parsers must target documented porcelain/machine-readable formats:
  - `status --porcelain` (v1)
  - `worktree list --porcelain`
  - `branch --format=%(refname:short)`
  - `merge-tree` (plain text)
- Parsers must tolerate extra whitespace and unknown status codes gracefully.
- Parsers must not depend on localized git output. Use `LANG=C` where necessary.

### 3. Error Message Heuristics

- Some errors are detected by parsing stderr (e.g. "already exists", "not found").
- These heuristics must be covered by unit tests with sample stderr output.
- When git introduces new error messages, heuristics may need updates.

### 4. Timeout Policy

- All git commands have a default timeout of 60 seconds.
- Long-running commands (fetch, push, clone) may accept an override.
- Timeout must be documented in the function's doc comment.

## Build & Test

Local verification commands:

```bash
# Run all tests (unit + integration + doc-tests)
cargo test --all-features

# Lint and type-check
cargo clippy --all-targets --all-features
cargo check --all-targets --all-features

# Documentation (must build without warnings)
cargo doc --no-deps --all-features

# Format check
cargo fmt --check
```

CI runs: `cargo test --all-features`, `cargo clippy --all-targets --all-features`,
`cargo doc --no-deps --all-features`.

## Clippy Lint Policy

Enabled lints live in `src/lib.rs` via `#![warn(...)]`. They produce warnings during
compilation without breaking the build, but must be addressed before merge.

### Tier 1 — Must-fix (high value, low noise)

| Lint | What it catches | Rationale |
|---|---|---|
| `clippy::await_holding_lock` | `std::sync::Mutex` or `RefCell` held across `.await` | Prevents executor blocking and deadlocks in async code |
| `clippy::dbg_macro` | `dbg!()` left in committed code | Debugging macros must not reach production |
| `clippy::wildcard_imports` | `use module::*` outside preludes/tests | Keeps imports explicit and traceable |
| `clippy::unused_async` | `async fn` that does not `.await` anything | Removes unnecessary async overhead |

### Tier 2 — Recommended (address when touching nearby code)

| Lint | What it catches |
|---|---|
| `clippy::missing_panics_doc` | `expect()` / `panic!()` without doc comment explaining the invariant |
| `clippy::cast_sign_loss` | `as u64` from signed types (e.g. `i64 as u64`) |
| `clippy::manual_strip` | Manual prefix/suffix checking instead of `strip_prefix`/`strip_suffix` |

### Tier 3 — Already clean (maintain zero violations)

- Zero `TODO` / `FIXME` / `HACK` comments in production code
- All public types implement `Debug`
- Zero `unsafe` blocks (currently 0 in `src/`)

## Editing Rules

- When modifying code, check whether a subdirectory has its own `AGENTS.md` for more specific guidance.
- Keep deeper-directory rules as overrides to these root rules.
- Update this file if you change any convention it describes.
