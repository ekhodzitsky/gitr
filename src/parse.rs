use crate::error::GitError;
use crate::types::{GitMergeResult, GitStatus, GitWorktree};
use std::path::PathBuf;

/// Parse `git status --porcelain` output.
pub fn parse_status(output: &str) -> Result<GitStatus, GitError> {
    let mut status = GitStatus::default();
    for line in output.lines() {
        if line.len() < 3 {
            continue;
        }
        let code = &line[..2];
        let path = line[3..].to_string();
        match code.as_bytes() {
            // Untracked
            [b'?', b'?'] => status.untracked.push(path),
            // Ignored (skip)
            [b'!', b'!'] => {}
            // Staged changes (index side is not space or ?)
            _ => {
                if code.as_bytes()[0] != b' ' && code.as_bytes()[0] != b'?' {
                    status.staged.push(path.clone());
                }
                if code.as_bytes()[1] != b' ' && code.as_bytes()[1] != b'?' {
                    status.unstaged.push(path);
                }
            }
        }
    }
    Ok(status)
}

/// Parse `git worktree list --porcelain` output.
pub fn parse_worktrees(output: &str) -> Result<Vec<GitWorktree>, GitError> {
    let mut worktrees = Vec::new();
    let mut current_path = None;
    let mut current_branch = None;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(path) = current_path.take() {
                worktrees.push(GitWorktree {
                    path,
                    branch: current_branch.take().unwrap_or_default(),
                });
            }
            current_path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            let branch = rest.trim_start_matches("refs/heads/");
            current_branch = Some(branch.to_string());
        }
    }

    if let Some(path) = current_path.take() {
        worktrees.push(GitWorktree {
            path,
            branch: current_branch.take().unwrap_or_default(),
        });
    }

    Ok(worktrees)
}

/// Parse `git branch --format=%(refname:short)` output.
pub fn parse_branches(output: &str) -> Result<Vec<String>, GitError> {
    Ok(output
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

/// Parse `git merge-tree` output for conflict detection.
pub fn parse_merge_tree(output: &str) -> Result<GitMergeResult, GitError> {
    let mut result = GitMergeResult::default();
    let mut files = Vec::new();

    for line in output.lines() {
        if line.contains("conflict") {
            result.has_conflicts = true;
        }
        if line.starts_with("merged\t") || line.starts_with("added\t") {
            if let Some(file) = line.split('\t').nth(1) {
                files.push(file.to_string());
            }
        }
    }

    if result.has_conflicts {
        result.conflict_files = files;
    }

    Ok(result)
}
