use crate::error::GitError;
use crate::types::{GitLogEntry, GitMergeResult, GitRemote, GitStatus, GitWorktree};
use std::path::PathBuf;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Statistics from `git diff --shortstat`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiffShortstat {
    /// Number of files changed.
    pub files_changed: u64,
    /// Number of insertions.
    pub insertions: u64,
    /// Number of deletions.
    pub deletions: u64,
}

/// Parse `git status --porcelain` output.
pub fn parse_status(stdout: &str) -> Result<GitStatus, GitError> {
    parse_status_impl(stdout, false)
}

/// Parse `git status --porcelain -z` output (null-delimited).
pub fn parse_status_z(stdout: &str) -> Result<GitStatus, GitError> {
    parse_status_impl(stdout, true)
}

fn parse_status_impl(stdout: &str, nul_terminated: bool) -> Result<GitStatus, GitError> {
    let mut status = GitStatus::default();

    if nul_terminated {
        let mut parts = stdout.split('\0');
        while let Some(entry) = parts.next() {
            if entry.len() < 3 {
                continue;
            }
            let idx = entry.as_bytes()[0];
            let wt = entry.as_bytes()[1];
            let path = &entry[3..];

            // Rename/copy: original path, next part is new path
            if idx == b'R' || idx == b'C' || wt == b'R' || wt == b'C' {
                let new_path = parts.next().unwrap_or(path).to_string();
                if idx != b' ' {
                    status.staged.push(new_path);
                } else if wt != b' ' {
                    status.unstaged.push(new_path);
                }
                continue;
            }

            if idx == b'?' && wt == b'?' {
                status.untracked.push(path.to_string());
            } else if idx == b'!' && wt == b'!' {
                // ignored
            } else {
                if idx != b' ' {
                    status.staged.push(path.to_string());
                }
                if wt != b' ' {
                    status.unstaged.push(path.to_string());
                }
            }
        }
    } else {
        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }
            let idx = line.as_bytes().first().copied().unwrap_or(b' ');
            let wt = line.as_bytes().get(1).copied().unwrap_or(b' ');
            let rest = &line[3..];
            // Rename/copy lines: "XY old_path -> new_path"
            let path = if let Some(arrow) = rest.find(" -> ") {
                rest[arrow + 4..].to_string()
            } else {
                rest.to_string()
            };

            if idx == b'?' && wt == b'?' {
                status.untracked.push(path);
            } else if idx == b'!' && wt == b'!' {
                // ignored
            } else if idx != b' ' {
                status.staged.push(path);
            } else if wt != b' ' {
                status.unstaged.push(path);
            }
        }
    }

    Ok(status)
}

/// Parse `git branch --format='%(refname:short)'` output.
pub fn parse_branches(stdout: &str) -> Result<Vec<String>, GitError> {
    Ok(stdout
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Parse `git worktree list --porcelain` output.
pub fn parse_worktrees(stdout: &str) -> Result<Vec<GitWorktree>, GitError> {
    let mut worktrees = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in stdout.lines() {
        if line.is_empty() {
            if let (Some(path), branch) = (current_path.take(), current_branch.take()) {
                worktrees.push(GitWorktree {
                    path: PathBuf::from(path),
                    branch: branch.unwrap_or_default(),
                });
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(path.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
        } else if line.starts_with("detached") {
            current_branch = Some("(detached)".to_string());
        }
    }
    if let (Some(path), branch) = (current_path.take(), current_branch.take()) {
        worktrees.push(GitWorktree {
            path: PathBuf::from(path),
            branch: branch.unwrap_or_default(),
        });
    }
    Ok(worktrees)
}

/// Parse `git log --format='%H|%s|%an|%at'` output.
#[allow(dead_code)]
pub fn parse_log(stdout: &str) -> Result<Vec<GitLogEntry>, GitError> {
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() != 4 {
            continue;
        }
        let timestamp = parts[3]
            .parse::<i64>()
            .map_err(|e| GitError::Parse(format!("invalid timestamp in log line '{line}': {e}")))?;
        entries.push(GitLogEntry {
            sha: parts[0].to_string(),
            short_sha: parts[0][..7.min(parts[0].len())].to_string(),
            message: parts[1].to_string(),
            author: parts[2].to_string(),
            timestamp: timestamp.to_string(),
        });
    }
    Ok(entries)
}

/// Parse `git remote -v` output.
#[allow(dead_code)]
pub fn parse_remotes(stdout: &str) -> Result<Vec<GitRemote>, GitError> {
    let mut remotes = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            remotes.push(GitRemote {
                name: parts[0].to_string(),
                url: parts[1].to_string(),
            });
        }
    }
    Ok(remotes)
}

/// Parse `git merge-tree` output for conflicts.
pub fn parse_merge_tree(stdout: &str) -> Result<GitMergeResult, GitError> {
    let mut result = GitMergeResult::default();
    for line in stdout.lines() {
        if line.starts_with("conflict") || line.contains("CONFLICT") {
            result.has_conflicts = true;
        }
        if let Some(path) = line.strip_prefix("conflict ") {
            result.has_conflicts = true;
            result.conflict_files.push(path.to_string());
        }
        if line.starts_with("merged ") || line.starts_with("added ") {
            // successful merge entries — no-op
        }
        // First non-empty line may be the tree OID (40 hex chars)
        if result.tree_oid.is_none()
            && line.len() == 40
            && line.chars().all(|c| c.is_ascii_hexdigit())
        {
            result.tree_oid = Some(line.to_string());
        }
        // Extract conflict file from "CONFLICT (content): Merge conflict in file.txt"
        if line.contains("CONFLICT") {
            if let Some(rest) = line.split("Merge conflict in ").nth(1) {
                result.conflict_files.push(rest.to_string());
            }
            // Handle other git conflict types:
            // CONFLICT (rename/delete): ...
            // CONFLICT (modify/delete): ...
            // CONFLICT (delete/modify): ...
            // CONFLICT (rename/rename): ...
            // CONFLICT (directory/file): ...
            if let Some(rest) = line.split("CONFLICT (rename/delete): ").nth(1) {
                result.conflict_files.push(rest.to_string());
            }
            if let Some(rest) = line.split("CONFLICT (modify/delete): ").nth(1) {
                result.conflict_files.push(rest.to_string());
            }
            if let Some(rest) = line.split("CONFLICT (delete/modify): ").nth(1) {
                result.conflict_files.push(rest.to_string());
            }
            if let Some(rest) = line.split("CONFLICT (rename/rename): ").nth(1) {
                result.conflict_files.push(rest.to_string());
            }
            if let Some(rest) = line.split("CONFLICT (directory/file): ").nth(1) {
                result.conflict_files.push(rest.to_string());
            }
        }
    }
    // Deduplicate
    result.conflict_files.sort();
    result.conflict_files.dedup();
    Ok(result)
}

/// Parse `git diff --numstat` or plain diff output to check for non-empty diff.
#[allow(dead_code)]
pub fn parse_has_diff(stdout: &str) -> bool {
    !stdout.trim().is_empty()
}

/// Parse `git diff --shortstat` output.
pub fn parse_diff_shortstat(stdout: &str) -> Result<DiffShortstat, GitError> {
    let mut stat = DiffShortstat::default();
    let line = stdout.trim();
    if line.is_empty() {
        return Ok(stat);
    }

    for part in line.split(',') {
        let part = part.trim();
        if let Some(n) = part.strip_suffix(" files changed") {
            stat.files_changed = n
                .parse()
                .map_err(|e| GitError::Parse(format!("invalid files changed: {e}")))?;
        } else if let Some(n) = part.strip_suffix(" file changed") {
            stat.files_changed = n
                .parse()
                .map_err(|e| GitError::Parse(format!("invalid file changed: {e}")))?;
        } else if let Some(n) = part.strip_suffix(" insertions(+)") {
            stat.insertions = n
                .parse()
                .map_err(|e| GitError::Parse(format!("invalid insertions: {e}")))?;
        } else if let Some(n) = part.strip_suffix(" insertion(+)") {
            stat.insertions = n
                .parse()
                .map_err(|e| GitError::Parse(format!("invalid insertion: {e}")))?;
        } else if let Some(n) = part.strip_suffix(" deletions(-)") {
            stat.deletions = n
                .parse()
                .map_err(|e| GitError::Parse(format!("invalid deletions: {e}")))?;
        } else if let Some(n) = part.strip_suffix(" deletion(-)") {
            stat.deletions = n
                .parse()
                .map_err(|e| GitError::Parse(format!("invalid deletion: {e}")))?;
        }
    }

    Ok(stat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_status_empty() {
        let s = parse_status("").unwrap();
        assert!(s.staged.is_empty());
        assert!(s.unstaged.is_empty());
        assert!(s.untracked.is_empty());
    }

    #[test]
    fn test_parse_status_mixed() {
        let input = " M src/main.rs\nM  src/lib.rs\n?? new.txt\n D old.rs";
        let s = parse_status(input).unwrap();
        assert_eq!(s.staged, vec!["src/lib.rs"]);
        assert_eq!(s.unstaged, vec!["src/main.rs", "old.rs"]);
        assert_eq!(s.untracked, vec!["new.txt"]);
    }

    #[test]
    fn test_parse_status_rename() {
        let input = "R  old.txt -> new.txt\n";
        let s = parse_status(input).unwrap();
        assert_eq!(s.staged, vec!["new.txt"]);
    }

    #[test]
    fn test_parse_status_z_basic() {
        let input = " M file.txt\0?? untracked.txt\0";
        let s = parse_status_z(input).unwrap();
        assert_eq!(s.unstaged, vec!["file.txt"]);
        assert_eq!(s.untracked, vec!["untracked.txt"]);
    }

    #[test]
    fn test_parse_status_z_with_spaces() {
        let input = " M path with spaces.txt\0?? another file.txt\0";
        let s = parse_status_z(input).unwrap();
        assert_eq!(s.unstaged, vec!["path with spaces.txt"]);
        assert_eq!(s.untracked, vec!["another file.txt"]);
    }

    #[test]
    fn test_parse_status_z_rename() {
        let input = "R  old.txt\0new.txt\0";
        let s = parse_status_z(input).unwrap();
        assert_eq!(s.staged, vec!["new.txt"]);
    }

    #[test]
    fn test_parse_branches() {
        let input = "main\nfeature/x\n  \n";
        let b = parse_branches(input).unwrap();
        assert_eq!(b, vec!["main", "feature/x"]);
    }

    #[test]
    fn test_parse_worktrees() {
        let input = "worktree /tmp/wt1\nbranch main\n\nworktree /tmp/wt2\ndetached\n";
        let w = parse_worktrees(input).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].path, PathBuf::from("/tmp/wt1"));
        assert_eq!(w[0].branch, "main");
        assert_eq!(w[1].path, PathBuf::from("/tmp/wt2"));
        assert_eq!(w[1].branch, "(detached)");
    }

    #[test]
    fn test_parse_log() {
        let input = "abc123|msg|author|1700000000\ndef456|msg2|author2|1700000001\n";
        let l = parse_log(input).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].sha, "abc123");
        assert_eq!(l[0].message, "msg");
        assert_eq!(l[0].author, "author");
        assert_eq!(l[0].timestamp, "1700000000");
    }

    #[test]
    fn test_parse_log_invalid_timestamp() {
        let input = "abc123|msg|author|bad\n";
        let err = parse_log(input).unwrap_err();
        assert!(matches!(err, GitError::Parse(_)));
    }

    #[test]
    fn test_parse_remotes() {
        let input = "origin  https://github.com/foo/bar.git (fetch)\norigin  https://github.com/foo/bar.git (push)\n";
        let r = parse_remotes(input).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "origin");
        assert_eq!(r[0].url, "https://github.com/foo/bar.git");
    }

    #[test]
    fn test_parse_merge_tree_clean() {
        let input = "aabbccdd00112233445566778899aabbccdd0011\nmerged src/main.rs\n";
        let m = parse_merge_tree(input).unwrap();
        assert!(!m.has_conflicts);
        assert!(m.conflict_files.is_empty());
        assert_eq!(
            m.tree_oid,
            Some("aabbccdd00112233445566778899aabbccdd0011".to_string())
        );
    }

    #[test]
    fn test_parse_merge_tree_conflicts() {
        let input = "conflict src/main.rs\nconflict src/lib.rs\n";
        let m = parse_merge_tree(input).unwrap();
        assert!(m.has_conflicts);
        assert_eq!(m.conflict_files, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_parse_merge_tree_conflict_line() {
        let input = "CONFLICT (content): Merge conflict in src/main.rs\n";
        let m = parse_merge_tree(input).unwrap();
        assert!(m.has_conflicts);
        assert_eq!(m.conflict_files, vec!["src/main.rs"]);
    }

    #[test]
    fn test_parse_merge_tree_rename_delete() {
        let input = "CONFLICT (rename/delete): old.txt\n";
        let m = parse_merge_tree(input).unwrap();
        assert!(m.has_conflicts);
        assert_eq!(m.conflict_files, vec!["old.txt"]);
    }

    #[test]
    fn test_parse_merge_tree_modify_delete() {
        let input = "CONFLICT (modify/delete): modified.txt\n";
        let m = parse_merge_tree(input).unwrap();
        assert!(m.has_conflicts);
        assert_eq!(m.conflict_files, vec!["modified.txt"]);
    }

    #[test]
    fn test_parse_merge_tree_delete_modify() {
        let input = "CONFLICT (delete/modify): deleted.txt\n";
        let m = parse_merge_tree(input).unwrap();
        assert!(m.has_conflicts);
        assert_eq!(m.conflict_files, vec!["deleted.txt"]);
    }

    #[test]
    fn test_parse_merge_tree_rename_rename() {
        let input = "CONFLICT (rename/rename): renamed.txt\n";
        let m = parse_merge_tree(input).unwrap();
        assert!(m.has_conflicts);
        assert_eq!(m.conflict_files, vec!["renamed.txt"]);
    }

    #[test]
    fn test_parse_merge_tree_directory_file() {
        let input = "CONFLICT (directory/file): dirfile.txt\n";
        let m = parse_merge_tree(input).unwrap();
        assert!(m.has_conflicts);
        assert_eq!(m.conflict_files, vec!["dirfile.txt"]);
    }

    #[test]
    fn test_parse_has_diff() {
        assert!(parse_has_diff("1\t2\tfile.rs\n"));
        assert!(!parse_has_diff("   \n"));
        assert!(!parse_has_diff(""));
    }

    #[test]
    fn test_parse_diff_shortstat_empty() {
        let s = parse_diff_shortstat("").unwrap();
        assert_eq!(s, DiffShortstat::default());
    }

    #[test]
    fn test_parse_diff_shortstat_full() {
        let input = " 2 files changed, 10 insertions(+), 5 deletions(-)";
        let s = parse_diff_shortstat(input).unwrap();
        assert_eq!(s.files_changed, 2);
        assert_eq!(s.insertions, 10);
        assert_eq!(s.deletions, 5);
    }

    #[test]
    fn test_parse_diff_shortstat_single_file() {
        let input = " 1 file changed, 3 insertions(+)";
        let s = parse_diff_shortstat(input).unwrap();
        assert_eq!(s.files_changed, 1);
        assert_eq!(s.insertions, 3);
        assert_eq!(s.deletions, 0);
    }

    #[test]
    fn test_parse_diff_shortstat_only_deletions() {
        let input = " 1 file changed, 1 deletion(-)";
        let s = parse_diff_shortstat(input).unwrap();
        assert_eq!(s.files_changed, 1);
        assert_eq!(s.insertions, 0);
        assert_eq!(s.deletions, 1);
    }
}
