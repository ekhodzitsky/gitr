# Contributing to `gitr`

Thank you for your interest in contributing! This document covers the basics.

## Getting started

1. Fork the repository.
2. Clone your fork.
3. Ensure you have Rust 1.80+ installed.
4. Ensure `git` is in your `PATH` (required for integration tests).

## Building

```bash
cargo build --all-features
```

## Running tests

```bash
# All tests (unit + integration + doc-tests)
cargo test --all-features

# Coverage report (requires cargo-tarpaulin)
cargo tarpaulin --all-features --fail-under 85
```

## Code quality

All PRs must pass these checks:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps --all-features
```

## Pull request process

1. Create a branch from `main`.
2. Make your changes.
3. Add tests for new functionality.
4. Update `CHANGELOG.md` under `## [Unreleased]`.
5. Ensure all quality checks pass.
6. Open a PR with a clear description.

## Release discipline

- Every user-visible change needs a `CHANGELOG.md` entry in the same PR.
- Public API changes need updates to `README.md` and `SPEC.md`.
- Version bumps happen in dedicated release PRs, not feature PRs.
