//! # gitr
//!
//! Async typed git CLI wrapper for agents and automation.
//!
//! ## Quick start
//!
//! ```no_run
//! use gitr::Repository;
//!
//! # async fn example() -> Result<(), gitr::Error> {
//! let repo = Repository::open(".").await?;
//! let branch = repo.current_branch().await?;
//! # Ok(())
//! # }
//! ```

#![warn(
    clippy::await_holding_lock,
    clippy::dbg_macro,
    clippy::wildcard_imports,
    clippy::unused_async,
    clippy::missing_panics_doc,
    clippy::cast_sign_loss,
    clippy::manual_strip,
    missing_docs
)]

mod api;
mod cache;
mod circuit;
mod command;
mod error;
/// Git CLI output parsers.
pub mod parse;
mod repo;
mod types;

pub use api::GitApi;
pub use cache::Cache;
pub use circuit::CircuitBreaker;
pub use command::CommandOutput;

#[cfg(any(test, feature = "test-utils"))]
pub use command::ScriptedRunner;
pub use error::GitError as Error;
pub use repo::Repository;
pub use types::{
    ApplyReport, BisectResult, BisectState, BlameLine, CherryPickOptions, CloneOptions,
    CommitOptions, DiffHunk, DiffLine, DiffLineKind, FetchOptions, FileDiff, GitAttr, GitCommit,
    GitGrepResult, GitLfsFile, GitLogEntry, GitMergeResult, GitNote, GitRemote, GitStash,
    GitStatus, GitSubmodule, GitTag, GitVerification, GitVersion, GitWorktree, Hook, HookOutput,
    IndexEntry, MergeOptions, ObjectContent, ObjectKind, Oid, Patch, PushOptions, RebaseOptions,
    ReflogEntry, ResetMode, TreeEntry,
};
