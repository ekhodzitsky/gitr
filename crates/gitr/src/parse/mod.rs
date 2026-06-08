use crate::error::GitError;
use crate::types::{
    BlameLine, DiffHunk, DiffLine, DiffLineKind, FileDiff, GitLogEntry, GitMergeResult, GitRemote,
    GitStatus, GitSubmodule, GitVersion, GitWorktree,
};
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

/// Check whether the detected git version is newer than what parsers were tested against.
#[cfg_attr(feature = "tracing", tracing::instrument)]
pub fn check_git_version_compat(version: GitVersion) {
    let _ = version;
    #[cfg(feature = "tracing")]
    if version
        > (GitVersion {
            major: 2,
            minor: 45,
            patch: 0,
        })
    {
        tracing::warn!(%version, "git version is newer than tested; parsers may need updates");
    }
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
            } else {
                if idx != b' ' {
                    status.staged.push(path.clone());
                }
                if wt != b' ' {
                    status.unstaged.push(path);
                }
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

/// Parse a single `git log --format='%H|%s|%an|%at'` line.
pub fn parse_log_line(line: &str) -> Result<GitLogEntry, GitError> {
    let parts: Vec<&str> = line.splitn(4, '|').collect();
    if parts.len() != 4 {
        return Err(GitError::Parse(format!("invalid log line: {line}")));
    }
    let timestamp = parts[3]
        .parse::<i64>()
        .map_err(|e| GitError::Parse(format!("invalid timestamp in log line '{line}': {e}")))?;
    Ok(GitLogEntry {
        sha: parts[0].to_string(),
        short_sha: parts[0][..7.min(parts[0].len())].to_string(),
        message: parts[1].to_string(),
        author: parts[2].to_string(),
        timestamp: timestamp.to_string(),
    })
}

/// Parse `git log --format='%H|%s|%an|%at'` output.
pub fn parse_log(stdout: &str) -> Result<Vec<GitLogEntry>, GitError> {
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        entries.push(parse_log_line(line)?);
    }
    Ok(entries)
}

/// Parse `git remote -v` output.
pub fn parse_remotes(stdout: &str) -> Result<Vec<GitRemote>, GitError> {
    let mut remotes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && seen.insert(parts[0]) {
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

/// Parse `git submodule status` output.
pub fn parse_submodules(stdout: &str) -> Result<Vec<GitSubmodule>, GitError> {
    let mut subs = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut chars = line.chars();
        let prefix = chars.next().unwrap_or(' ');
        let after_prefix = if prefix == ' ' || prefix == '+' || prefix == '-' || prefix == 'U' {
            chars.as_str()
        } else {
            line
        };
        if after_prefix.len() < 40 {
            continue;
        }
        let sha = &after_prefix[..40];
        let rest = after_prefix[40..].trim_start();
        let (path, describe) = if let Some(start) = rest.rfind(" (") {
            if rest.ends_with(')') {
                (
                    rest[..start].to_string(),
                    Some(rest[start + 2..rest.len() - 1].to_string()),
                )
            } else {
                (rest.to_string(), None)
            }
        } else {
            (rest.to_string(), None)
        };
        subs.push(GitSubmodule {
            sha: sha.to_string(),
            path,
            describe,
            dirty: prefix == '+',
            uninitialized: prefix == '-',
        });
    }
    Ok(subs)
}

/// Parse `git grep -n` output.
pub fn parse_grep(stdout: &str) -> Result<Vec<crate::types::GitGrepResult>, GitError> {
    let mut results = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() != 3 {
            continue;
        }
        let line_num = parts[1]
            .parse::<u32>()
            .map_err(|e| GitError::Parse(format!("invalid grep line number '{line}': {e}")))?;
        results.push(crate::types::GitGrepResult {
            path: parts[0].to_string(),
            line: line_num,
            text: parts[2].to_string(),
        });
    }
    Ok(results)
}

/// Parse unified diff output (`git diff` or `git diff --cached`) into a list of
/// per-file diffs.
///
/// Tolerates binary diffs, mode changes, and renames. Unknown extended header
/// lines are skipped.
pub fn parse_diff(stdout: &str) -> Result<Vec<FileDiff>, GitError> {
    let lines: Vec<&str> = stdout.lines().collect();
    let mut files = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Look for a diff --git header.
        let line = lines[i];
        if !line.starts_with("diff --git ") {
            i += 1;
            continue;
        }

        // Parse the diff --git line: "diff --git a/old b/new"
        let git_line = line;
        let after_prefix = &git_line["diff --git ".len()..];
        let parts: Vec<&str> = after_prefix.split_whitespace().collect();
        let (mut old_path, mut new_path) = (None, None);
        if parts.len() == 2 {
            let old_p = strip_a_prefix(parts[0]);
            let new_p = strip_b_prefix(parts[1]);
            old_path = Some(old_p.to_string());
            new_path = Some(new_p.to_string());
        }

        let mut is_binary = false;
        let mut mode_changed = false;
        let mut old_mode = None;
        let mut new_mode = None;
        let mut hunks = Vec::new();

        i += 1;

        // Extended headers (index, old mode, new mode, rename, similarity, etc.)
        while i < lines.len()
            && !lines[i].starts_with("--- ")
            && !lines[i].starts_with("diff --git ")
            && !lines[i].starts_with("@@ ")
        {
            let ext = lines[i];
            if ext.starts_with("index ") {
                // skip
            } else if let Some(mode) = ext.strip_prefix("old mode ") {
                mode_changed = true;
                old_mode = Some(mode.to_string());
            } else if let Some(mode) = ext.strip_prefix("new mode ") {
                mode_changed = true;
                new_mode = Some(mode.to_string());
            } else if let Some(path) = ext.strip_prefix("rename from ") {
                old_path = Some(path.to_string());
            } else if let Some(path) = ext.strip_prefix("rename to ") {
                new_path = Some(path.to_string());
            } else if ext.starts_with("similarity index ")
                || ext.starts_with("dissimilarity index ")
            {
                // skip
            } else if ext.starts_with("Binary files ") && ext.contains(" differ") {
                is_binary = true;
            }
            i += 1;
        }

        // --- / +++ lines
        if i < lines.len() && lines[i].starts_with("--- ") {
            let old_raw = &lines[i][4..];
            if old_raw == "/dev/null" {
                old_path = None;
            } else if old_path.is_none() {
                old_path = Some(strip_a_prefix(old_raw).to_string());
            }
            i += 1;
        }
        if i < lines.len() && lines[i].starts_with("+++ ") {
            let new_raw = &lines[i][4..];
            if new_raw == "/dev/null" {
                new_path = None;
            } else if new_path.is_none() {
                new_path = Some(strip_b_prefix(new_raw).to_string());
            }
            i += 1;
        }

        // Hunks
        while i < lines.len() && lines[i].starts_with("@@") {
            let hunk = parse_hunk(&lines, &mut i)?;
            hunks.push(hunk);
        }

        files.push(FileDiff {
            old_path,
            new_path,
            is_binary,
            mode_changed,
            old_mode,
            new_mode,
            hunks,
        });
    }

    Ok(files)
}

fn strip_a_prefix(s: &str) -> &str {
    s.strip_prefix("a/").unwrap_or(s)
}

fn strip_b_prefix(s: &str) -> &str {
    s.strip_prefix("b/").unwrap_or(s)
}

fn parse_hunk(lines: &[&str], idx: &mut usize) -> Result<DiffHunk, GitError> {
    let line = lines[*idx];
    // Format: @@ -old_start,old_lines +new_start,new_lines @@ section_header
    let hunk_header = &line[2..]; // strip leading "@@"
    let hunk_header = hunk_header.strip_prefix(" ").unwrap_or(hunk_header);
    let end_idx = hunk_header
        .find(" @@")
        .ok_or_else(|| GitError::Parse(format!("invalid hunk header: {line}")))?;
    let ranges = &hunk_header[..end_idx];
    let section = hunk_header[end_idx + 3..].to_string();

    let (old_range, new_range) = ranges
        .split_once(' ')
        .ok_or_else(|| GitError::Parse(format!("invalid hunk ranges: {ranges}")))?;

    let (old_start, old_lines) = parse_range(old_range)?;
    let (new_start, new_lines) = parse_range(new_range)?;

    *idx += 1;

    let mut diff_lines = Vec::new();
    while *idx < lines.len() {
        let l = lines[*idx];
        if l.starts_with("@@") || l.starts_with("diff --git ") {
            break;
        }
        if l == "\\ No newline at end of file" {
            diff_lines.push(DiffLine {
                kind: DiffLineKind::NoNewline,
                content: String::new(),
            });
        } else if l.is_empty() {
            // Empty line in hunk — treat as context with empty content.
            // In unified diff an empty context line still starts with a space,
            // but if the line is literally "" that can't happen. Skip it
            // unless it's part of the hunk (this shouldn't occur in well-formed
            // diff output).
            break;
        } else {
            let first = l.as_bytes()[0];
            let content = &l[1..];
            let kind = match first {
                b' ' => DiffLineKind::Context,
                b'-' => DiffLineKind::Deletion,
                b'+' => DiffLineKind::Insertion,
                _ => {
                    // Unexpected line in hunk — stop parsing this hunk.
                    break;
                }
            };
            diff_lines.push(DiffLine {
                kind,
                content: content.to_string(),
            });
        }
        *idx += 1;
    }

    Ok(DiffHunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        section,
        lines: diff_lines,
    })
}

fn parse_range(range: &str) -> Result<(usize, usize), GitError> {
    let rest = range
        .strip_prefix('-')
        .or_else(|| range.strip_prefix('+'))
        .ok_or_else(|| GitError::Parse(format!("invalid range prefix: {range}")))?;
    if let Some((start, count)) = rest.split_once(',') {
        let start = start
            .parse::<usize>()
            .map_err(|e| GitError::Parse(format!("invalid range start '{start}': {e}")))?;
        let count = count
            .parse::<usize>()
            .map_err(|e| GitError::Parse(format!("invalid range count '{count}': {e}")))?;
        Ok((start, count))
    } else {
        let start = rest
            .parse::<usize>()
            .map_err(|e| GitError::Parse(format!("invalid range '{rest}': {e}")))?;
        Ok((start, 1))
    }
}

/// Parse `git blame --line-porcelain` output into a list of per-line entries.
pub fn parse_blame(stdout: &str) -> Result<Vec<BlameLine>, GitError> {
    let lines: Vec<&str> = stdout.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // First line of a block: <sha> <orig-line> <final-line> <group-lines>
        let header = lines[i];
        let sha = header
            .split_whitespace()
            .next()
            .ok_or_else(|| GitError::Parse(format!("invalid blame header: {header}")))?
            .to_string();
        i += 1;

        let mut author = String::new();
        let mut author_mail = String::new();
        let mut author_time = String::new();
        let mut content = String::new();

        while i < lines.len() {
            let line = lines[i];
            if let Some(rest) = line.strip_prefix("author ") {
                author = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("author-mail ") {
                author_mail = rest.trim_matches('<').trim_matches('>').to_string();
            } else if let Some(rest) = line.strip_prefix("author-time ") {
                author_time = rest.to_string();
            } else if let Some(rest) = line.strip_prefix('\t') {
                content = rest.to_string();
                i += 1;
                break;
            } else if line.len() >= 40 && line.as_bytes()[0].is_ascii_hexdigit() {
                // Looks like the start of the next blame block (new SHA header).
                break;
            }
            i += 1;
        }

        result.push(BlameLine {
            commit: sha,
            author,
            author_mail,
            author_time,
            line_no: result.len() + 1,
            content,
        });
    }

    Ok(result)
}

/// Parse `git format-patch --stdout` output (mbox format) into a list of
/// structured patches.
pub fn parse_format_patch(stdout: &str) -> Result<Vec<crate::types::Patch>, GitError> {
    let mut patches = Vec::new();
    let lines: Vec<&str> = stdout.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // Look for mbox separator: "From <hash> Mon Sep 17 00:00:00 2001"
        if !lines[i].starts_with("From ") {
            i += 1;
            continue;
        }

        let mut commit = None;
        let mut from = None;
        let mut date = None;
        let mut subject = None;

        // Parse the "From " line for commit hash
        let from_line = lines[i];
        if let Some(hash_end) = from_line["From ".len()..].find(' ') {
            commit = Some(from_line["From ".len()..][..hash_end].to_string());
        }
        i += 1;

        // Parse headers until we hit the `---` separator or the diff
        while i < lines.len() {
            let line = lines[i];
            if line == "---" {
                i += 1;
                break;
            }
            if let Some(rest) = line.strip_prefix("From: ") {
                from = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("Date: ") {
                date = Some(rest.to_string());
            } else if let Some(rest) = line.strip_prefix("Subject: ") {
                subject = Some(rest.to_string());
            }
            i += 1;
        }

        // Collect diff lines until next "From " or EOF
        let mut diff_lines = Vec::new();
        while i < lines.len() && !lines[i].starts_with("From ") {
            diff_lines.push(lines[i]);
            i += 1;
        }

        let diff_text = diff_lines.join("\n");
        let diff = if diff_text.trim().is_empty() {
            Vec::new()
        } else {
            parse_diff(&diff_text)?
        };

        patches.push(crate::types::Patch {
            commit,
            subject,
            from,
            date,
            diff,
        });
    }

    Ok(patches)
}

/// Parse `git reflog show --format=%H|%an|%ae|%at|%gs|%gd` output.
pub fn parse_reflog(stdout: &str) -> Result<Vec<crate::types::ReflogEntry>, GitError> {
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(6, '|').collect();
        if parts.len() != 6 {
            continue;
        }
        entries.push(crate::types::ReflogEntry {
            commit: parts[0].to_string(),
            author: parts[1].to_string(),
            author_mail: parts[2].to_string(),
            timestamp: parts[3].to_string(),
            subject: parts[4].to_string(),
            designator: parts[5].to_string(),
        });
    }
    Ok(entries)
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
    fn test_parse_status_staged_and_unstaged() {
        let input = "MM file.rs\n";
        let s = parse_status(input).unwrap();
        assert_eq!(s.staged, vec!["file.rs"]);
        assert_eq!(s.unstaged, vec!["file.rs"]);
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
        assert_eq!(r.len(), 1);
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

    #[test]
    fn test_parse_submodules_clean() {
        let input = " 4a20283f8c33c1e9f12ab234a1e5b1e1e8e3e1e1 submodules/foo (v1.2.3)\n";
        let s = parse_submodules(input).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].sha, "4a20283f8c33c1e9f12ab234a1e5b1e1e8e3e1e1");
        assert_eq!(s[0].path, "submodules/foo");
        assert_eq!(s[0].describe, Some("v1.2.3".to_string()));
        assert!(!s[0].dirty);
        assert!(!s[0].uninitialized);
    }

    #[test]
    fn test_parse_submodules_dirty_and_uninit() {
        let input = "+4a20283f8c33c1e9f12ab234a1e5b1e1e8e3e1e1 submodules/bar (heads/main)\n-4a20283f8c33c1e9f12ab234a1e5b1e1e8e3e1e1 submodules/baz\n";
        let s = parse_submodules(input).unwrap();
        assert_eq!(s.len(), 2);
        assert!(s[0].dirty);
        assert!(!s[0].uninitialized);
        assert_eq!(s[0].describe, Some("heads/main".to_string()));
        assert!(s[1].uninitialized);
        assert!(!s[1].dirty);
        assert_eq!(s[1].describe, None);
    }

    #[test]
    fn test_parse_grep_basic() {
        let input = "src/main.rs:10:fn main() {}\nsrc/lib.rs:5:pub fn add() {}\n";
        let r = parse_grep(input).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].path, "src/main.rs");
        assert_eq!(r[0].line, 10);
        assert_eq!(r[0].text, "fn main() {}");
        assert_eq!(r[1].path, "src/lib.rs");
        assert_eq!(r[1].line, 5);
        assert_eq!(r[1].text, "pub fn add() {}");
    }

    #[test]
    fn test_parse_grep_colons_in_text() {
        let input = "a.txt:1:x: y: z\n";
        let r = parse_grep(input).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].text, "x: y: z");
    }

    #[test]
    fn test_parse_status_untracked() {
        let input = "?? untracked.txt\n";
        let s = parse_status(input).unwrap();
        assert!(s.staged.is_empty());
        assert!(s.unstaged.is_empty());
        assert_eq!(s.untracked, vec!["untracked.txt"]);
    }

    #[test]
    fn test_parse_status_copied() {
        let input = "C  old.txt -> new.txt\n";
        let s = parse_status(input).unwrap();
        assert_eq!(s.staged, vec!["new.txt"]);
    }

    #[test]
    fn test_parse_status_unmerged() {
        let input = "UU conflict.rs\n";
        let s = parse_status(input).unwrap();
        assert_eq!(s.staged, vec!["conflict.rs"]);
        assert_eq!(s.unstaged, vec!["conflict.rs"]);
    }

    #[test]
    fn test_parse_status_whitespace_only() {
        let s = parse_status("   \n\n").unwrap();
        assert!(s.staged.is_empty());
        assert!(s.unstaged.is_empty());
        assert!(s.untracked.is_empty());
    }

    #[test]
    fn test_parse_log_empty() {
        let l = parse_log("").unwrap();
        assert!(l.is_empty());
    }

    #[test]
    fn test_parse_log_single_no_trailing_newline() {
        let input = "abc123|msg|author|1700000000";
        let l = parse_log(input).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].sha, "abc123");
    }

    #[test]
    fn test_parse_log_line_ok() {
        let line = "abc123|hello|alice|1700000000";
        let entry = parse_log_line(line).unwrap();
        assert_eq!(entry.sha, "abc123");
        assert_eq!(entry.message, "hello");
        assert_eq!(entry.author, "alice");
        assert_eq!(entry.timestamp, "1700000000");
    }

    #[test]
    fn test_parse_log_line_missing_fields() {
        let line = "abc123|hello";
        let err = parse_log_line(line).unwrap_err();
        assert!(matches!(err, GitError::Parse(_)));
    }

    #[test]
    fn test_parse_grep_empty() {
        let r = parse_grep("").unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn test_parse_grep_invalid_line() {
        let input = "not a grep line\n";
        let r = parse_grep(input).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn test_parse_submodules_empty() {
        let s = parse_submodules("").unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn test_parse_diff_shortstat_zero_insertions() {
        let input = " 1 file changed, 0 insertions(+), 0 deletions(-)";
        let s = parse_diff_shortstat(input).unwrap();
        assert_eq!(s.files_changed, 1);
        assert_eq!(s.insertions, 0);
        assert_eq!(s.deletions, 0);
    }

    #[test]
    fn test_parse_diff_basic() {
        let input = r"diff --git a/file.txt b/file.txt
index 1234567..abcdefg 100644
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line one
-line two
+line two modified
 line three
";
        let files = parse_diff(input).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].new_path, Some("file.txt".to_string()));
        assert!(!files[0].is_binary);
        assert_eq!(files[0].hunks.len(), 1);
        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_lines, 3);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_lines, 3);
        assert_eq!(hunk.lines.len(), 4);
        assert_eq!(hunk.lines[0].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[0].content, "line one");
        assert_eq!(hunk.lines[1].kind, DiffLineKind::Deletion);
        assert_eq!(hunk.lines[1].content, "line two");
        assert_eq!(hunk.lines[2].kind, DiffLineKind::Insertion);
        assert_eq!(hunk.lines[2].content, "line two modified");
        assert_eq!(hunk.lines[3].kind, DiffLineKind::Context);
        assert_eq!(hunk.lines[3].content, "line three");
    }

    #[test]
    fn test_parse_diff_new_file() {
        let input = r"diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..1234567
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
";
        let files = parse_diff(input).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].old_path, None);
        assert_eq!(files[0].new_path, Some("new.txt".to_string()));
        assert_eq!(files[0].hunks.len(), 1);
        assert_eq!(files[0].hunks[0].lines.len(), 2);
        assert_eq!(files[0].hunks[0].lines[0].kind, DiffLineKind::Insertion);
    }

    #[test]
    fn test_parse_diff_deleted_file() {
        let input = r"diff --git a/old.txt b/old.txt
deleted file mode 100644
index 1234567..0000000
--- a/old.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-hello
-world
";
        let files = parse_diff(input).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].old_path, Some("old.txt".to_string()));
        assert_eq!(files[0].new_path, None);
        assert_eq!(files[0].hunks[0].lines.len(), 2);
        assert_eq!(files[0].hunks[0].lines[0].kind, DiffLineKind::Deletion);
    }

    #[test]
    fn test_parse_diff_binary() {
        let input = r"diff --git a/image.png b/image.png
Binary files a/image.png and b/image.png differ
";
        let files = parse_diff(input).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].is_binary);
        assert!(files[0].hunks.is_empty());
    }

    #[test]
    fn test_parse_diff_rename() {
        let input = r"diff --git a/old.txt b/new.txt
similarity index 100%
rename from old.txt
rename to new.txt
index 1234567..abcdefg 100644
--- a/old.txt
+++ b/new.txt
";
        let files = parse_diff(input).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].old_path, Some("old.txt".to_string()));
        assert_eq!(files[0].new_path, Some("new.txt".to_string()));
    }

    #[test]
    fn test_parse_diff_empty() {
        let files = parse_diff("").unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_parse_blame_basic() {
        let input = r"abc123 1 1 2
author Alice
author-mail <alice@example.com>
author-time 1700000000
	line one
def456 2 2 1
author Bob
author-mail <bob@example.com>
author-time 1700000001
	line two
";
        let lines = parse_blame(input).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].commit, "abc123");
        assert_eq!(lines[0].author, "Alice");
        assert_eq!(lines[0].author_mail, "alice@example.com");
        assert_eq!(lines[0].author_time, "1700000000");
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[0].content, "line one");
        assert_eq!(lines[1].commit, "def456");
        assert_eq!(lines[1].author, "Bob");
        assert_eq!(lines[1].content, "line two");
    }

    #[test]
    fn test_parse_blame_empty() {
        let lines = parse_blame("").unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn test_parse_format_patch_basic() {
        let input = r"From abc123 Mon Sep 17 00:00:00 2001
From: Alice <alice@example.com>
Date: Wed, 1 Jan 2020 00:00:00 +0000
Subject: [PATCH] fix bug

---
 file.txt | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)

diff --git a/file.txt b/file.txt
index 123..456 100644
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        let patches = parse_format_patch(input).unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].commit, Some("abc123".to_string()));
        assert_eq!(
            patches[0].from,
            Some("Alice <alice@example.com>".to_string())
        );
        assert_eq!(patches[0].subject, Some("[PATCH] fix bug".to_string()));
        assert_eq!(patches[0].diff.len(), 1);
        assert_eq!(patches[0].diff[0].new_path, Some("file.txt".to_string()));
    }

    #[test]
    fn test_parse_format_patch_empty() {
        let patches = parse_format_patch("").unwrap();
        assert!(patches.is_empty());
    }

    #[test]
    fn test_parse_reflog_basic() {
        let input = "abc123|Alice|alice@example.com|1700000000|commit: init|HEAD@{0}\ndef456|Bob|bob@example.com|1700000001|commit: second|HEAD@{1}\n";
        let entries = parse_reflog(input).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].commit, "abc123");
        assert_eq!(entries[0].author, "Alice");
        assert_eq!(entries[0].author_mail, "alice@example.com");
        assert_eq!(entries[0].timestamp, "1700000000");
        assert_eq!(entries[0].subject, "commit: init");
        assert_eq!(entries[0].designator, "HEAD@{0}");
    }

    #[test]
    fn test_parse_reflog_empty() {
        let entries = parse_reflog("").unwrap();
        assert!(entries.is_empty());
    }

    // -----------------------------------------------------------------------
    // Snapshot tests
    // -----------------------------------------------------------------------

    #[test]
    fn snapshot_parse_status_empty() {
        let s = parse_status("").unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_status_mixed() {
        let input = " M src/main.rs\nM  src/lib.rs\n?? new.txt\n D old.rs";
        let s = parse_status(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_status_rename() {
        let input = "R  old.txt -> new.txt\n";
        let s = parse_status(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_status_copy() {
        let input = "C  old.txt -> new.txt\n";
        let s = parse_status(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_status_untracked() {
        let input = "?? untracked.txt\n";
        let s = parse_status(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_status_unmerged() {
        let input = "UU conflict.rs\n";
        let s = parse_status(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_status_z_format() {
        let input = " M file.txt\0?? untracked.txt\0R  old.txt\0new.txt\0";
        let s = parse_status_z(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_diff_new_file() {
        let input = r"diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..1234567
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
";
        let files = parse_diff(input).unwrap();
        insta::assert_debug_snapshot!(files);
    }

    #[test]
    fn snapshot_parse_diff_deleted_file() {
        let input = r"diff --git a/old.txt b/old.txt
deleted file mode 100644
index 1234567..0000000
--- a/old.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-hello
-world
";
        let files = parse_diff(input).unwrap();
        insta::assert_debug_snapshot!(files);
    }

    #[test]
    fn snapshot_parse_diff_renamed_file() {
        let input = r"diff --git a/old.txt b/new.txt
similarity index 100%
rename from old.txt
rename to new.txt
index 1234567..abcdefg 100644
--- a/old.txt
+++ b/new.txt
";
        let files = parse_diff(input).unwrap();
        insta::assert_debug_snapshot!(files);
    }

    #[test]
    fn snapshot_parse_diff_binary() {
        let input = r"diff --git a/image.png b/image.png
Binary files a/image.png and b/image.png differ
";
        let files = parse_diff(input).unwrap();
        insta::assert_debug_snapshot!(files);
    }

    #[test]
    fn snapshot_parse_diff_mode_change() {
        let input = r"diff --git a/script.sh b/script.sh
old mode 100644
new mode 100755
index 1234567..1234567
--- a/script.sh
+++ b/script.sh
@@ -1 +1 @@
-old content
+new content
";
        let files = parse_diff(input).unwrap();
        insta::assert_debug_snapshot!(files);
    }

    #[test]
    fn snapshot_parse_diff_multi_hunk() {
        let input = r"diff --git a/file.txt b/file.txt
index 1234567..abcdefg 100644
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line one
-line two
+line two modified
 line three
@@ -10,3 +10,3 @@
 other line
-removed
+added
 last line
";
        let files = parse_diff(input).unwrap();
        insta::assert_debug_snapshot!(files);
    }

    #[test]
    fn snapshot_parse_blame_single_author() {
        let input = r"abc123 1 1 1
author Alice
author-mail <alice@example.com>
author-time 1700000000
	line one
";
        let lines = parse_blame(input).unwrap();
        insta::assert_debug_snapshot!(lines);
    }

    #[test]
    fn snapshot_parse_blame_multiple_authors() {
        let input = r"abc123 1 1 2
author Alice
author-mail <alice@example.com>
author-time 1700000000
	line one
def456 2 2 1
author Bob
author-mail <bob@example.com>
author-time 1700000001
	line two
";
        let lines = parse_blame(input).unwrap();
        insta::assert_debug_snapshot!(lines);
    }

    #[test]
    fn snapshot_parse_blame_empty_lines() {
        let input = "abc123 1 1 1\nauthor Alice\nauthor-mail <alice@example.com>\nauthor-time 1700000000\n\t\n";
        let lines = parse_blame(input).unwrap();
        insta::assert_debug_snapshot!(lines);
    }

    #[test]
    fn snapshot_parse_format_patch_single() {
        let input = r"From abc123 Mon Sep 17 00:00:00 2001
From: Alice <alice@example.com>
Date: Wed, 1 Jan 2020 00:00:00 +0000
Subject: [PATCH] fix bug

---
 file.txt | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)

diff --git a/file.txt b/file.txt
index 123..456 100644
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new
";
        let patches = parse_format_patch(input).unwrap();
        insta::assert_debug_snapshot!(patches);
    }

    #[test]
    fn snapshot_parse_format_patch_multiple() {
        let input = r"From abc123 Mon Sep 17 00:00:00 2001
From: Alice <alice@example.com>
Date: Wed, 1 Jan 2020 00:00:00 +0000
Subject: [PATCH] first patch

---
 file.txt | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)

diff --git a/file.txt b/file.txt
index 123..456 100644
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old
+new

From def456 Mon Sep 17 00:00:00 2001
From: Bob <bob@example.com>
Date: Thu, 2 Jan 2020 00:00:00 +0000
Subject: [PATCH] second patch

---
 other.txt | 1 +
 1 file changed, 1 insertion(+)

diff --git a/other.txt b/other.txt
new file mode 100644
index 0000000..789abcd
--- /dev/null
+++ b/other.txt
@@ -0,0 +1 @@
+hello
";
        let patches = parse_format_patch(input).unwrap();
        insta::assert_debug_snapshot!(patches);
    }

    #[test]
    fn snapshot_parse_format_patch_no_diff() {
        let input = r"From abc123 Mon Sep 17 00:00:00 2001
From: Alice <alice@example.com>
Date: Wed, 1 Jan 2020 00:00:00 +0000
Subject: [PATCH] empty patch

---
 0 files changed, 0 insertions(+), 0 deletions(-)
";
        let patches = parse_format_patch(input).unwrap();
        insta::assert_debug_snapshot!(patches);
    }

    #[test]
    fn snapshot_parse_reflog() {
        let input = "abc123|Alice|alice@example.com|1700000000|commit: init|HEAD@{0}\ndef456|Bob|bob@example.com|1700000001|commit: second|HEAD@{1}\n";
        let entries = parse_reflog(input).unwrap();
        insta::assert_debug_snapshot!(entries);
    }

    #[test]
    fn snapshot_parse_merge_tree_clean() {
        let input = "aabbccdd00112233445566778899aabbccdd0011\nmerged src/main.rs\n";
        let m = parse_merge_tree(input).unwrap();
        insta::assert_debug_snapshot!(m);
    }

    #[test]
    fn snapshot_parse_merge_tree_conflicts_all_types() {
        let input = r"conflict src/main.rs
CONFLICT (content): Merge conflict in file.txt
CONFLICT (rename/delete): old.txt
CONFLICT (modify/delete): mod.txt
CONFLICT (delete/modify): del.txt
CONFLICT (rename/rename): ren.txt
CONFLICT (directory/file): dir.txt
";
        let m = parse_merge_tree(input).unwrap();
        insta::assert_debug_snapshot!(m);
    }

    #[test]
    fn snapshot_parse_submodules_clean() {
        let input = " 4a20283f8c33c1e9f12ab234a1e5b1e1e8e3e1e1 submodules/foo (v1.2.3)\n";
        let s = parse_submodules(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_submodules_dirty() {
        let input = "+4a20283f8c33c1e9f12ab234a1e5b1e1e8e3e1e1 submodules/bar (heads/main)\n";
        let s = parse_submodules(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_submodules_uninitialized() {
        let input = "-4a20283f8c33c1e9f12ab234a1e5b1e1e8e3e1e1 submodules/baz\n";
        let s = parse_submodules(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_grep_basic() {
        let input = "src/main.rs:10:fn main() {}\nsrc/lib.rs:5:pub fn add() {}\n";
        let r = parse_grep(input).unwrap();
        insta::assert_debug_snapshot!(r);
    }

    #[test]
    fn snapshot_parse_grep_colons_in_text() {
        let input = "a.txt:1:x: y: z\n";
        let r = parse_grep(input).unwrap();
        insta::assert_debug_snapshot!(r);
    }

    #[test]
    fn snapshot_parse_grep_multiple_matches() {
        let input =
            "src/main.rs:1:fn foo() {}\nsrc/main.rs:2:fn bar() {}\nsrc/lib.rs:10:struct Baz;\n";
        let r = parse_grep(input).unwrap();
        insta::assert_debug_snapshot!(r);
    }

    #[test]
    fn snapshot_parse_diff_shortstat_empty() {
        let s = parse_diff_shortstat("").unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_diff_shortstat_full() {
        let input = " 2 files changed, 10 insertions(+), 5 deletions(-)";
        let s = parse_diff_shortstat(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_diff_shortstat_single_file() {
        let input = " 1 file changed, 3 insertions(+)";
        let s = parse_diff_shortstat(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_diff_shortstat_only_deletions() {
        let input = " 1 file changed, 1 deletion(-)";
        let s = parse_diff_shortstat(input).unwrap();
        insta::assert_debug_snapshot!(s);
    }

    #[test]
    fn snapshot_parse_log_normal() {
        let input = "abc123|msg|author|1700000000\ndef456|msg2|author2|1700000001\n";
        let l = parse_log(input).unwrap();
        insta::assert_debug_snapshot!(l);
    }

    #[test]
    fn snapshot_parse_log_line_normal() {
        let line = "abc123|hello|alice|1700000000";
        let entry = parse_log_line(line).unwrap();
        insta::assert_debug_snapshot!(entry);
    }

    #[test]
    fn snapshot_parse_log_line_invalid_timestamp() {
        let line = "abc123|msg|author|bad";
        let err = parse_log_line(line).unwrap_err();
        insta::assert_debug_snapshot!(err);
    }

    #[test]
    fn snapshot_parse_branches_normal() {
        let input = "main\nfeature/x\n  \n";
        let b = parse_branches(input).unwrap();
        insta::assert_debug_snapshot!(b);
    }

    #[test]
    fn snapshot_parse_branches_with_spaces() {
        let input = "my branch\nanother branch name\n";
        let b = parse_branches(input).unwrap();
        insta::assert_debug_snapshot!(b);
    }

    #[test]
    fn snapshot_parse_worktrees_normal() {
        let input = "worktree /tmp/wt1\nbranch main\n\nworktree /tmp/wt2\nbranch feature/x\n";
        let w = parse_worktrees(input).unwrap();
        insta::assert_debug_snapshot!(w);
    }

    #[test]
    fn snapshot_parse_worktrees_detached() {
        let input = "worktree /tmp/wt1\ndetached\n";
        let w = parse_worktrees(input).unwrap();
        insta::assert_debug_snapshot!(w);
    }

    #[test]
    fn snapshot_parse_remotes_normal() {
        let input = "origin  https://github.com/foo/bar.git (fetch)\norigin  https://github.com/foo/bar.git (push)\n";
        let r = parse_remotes(input).unwrap();
        insta::assert_debug_snapshot!(r);
    }

    #[test]
    fn test_parse_status_z_staged() {
        let input = "M  file.txt\0";
        let s = parse_status_z(input).unwrap();
        assert_eq!(s.staged, vec!["file.txt"]);
        assert!(s.unstaged.is_empty());
    }

    #[test]
    fn test_parse_status_z_unstaged_rename() {
        let input = " R old.txt\0new.txt\0";
        let s = parse_status_z(input).unwrap();
        assert_eq!(s.unstaged, vec!["new.txt"]);
    }

    #[test]
    fn test_parse_submodules_various() {
        // Empty line
        let s = parse_submodules("\n").unwrap();
        assert!(s.is_empty());
        // Short sha -> skip
        let s = parse_submodules(" 4a20283f src/foo\n").unwrap();
        assert!(s.is_empty());
        // No parentheses -> no describe
        let s = parse_submodules(" 4a20283f3b5e4a6a7b8c9d0e1f2a3b4c5d6e7f8g src/foo\n").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].path, "src/foo");
        assert!(s[0].describe.is_none());
    }

    #[test]
    fn test_parse_grep_empty_and_invalid() {
        let r = parse_grep("\nnot_a_grep_line\n").unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn test_parse_log_with_empty_line() {
        let input = "abc123|msg|author|1700000000\n\n";
        let l = parse_log(input).unwrap();
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn test_parse_diff_empty_line_in_hunk() {
        let input = r"diff --git a/f.txt b/f.txt
--- a/f.txt
+++ b/f.txt
@@ -1,2 +1,2 @@
 line one

 line two
";
        let files = parse_diff(input).unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_parse_diff_with_old_new_paths() {
        let input = r"diff --git a/old.txt b/new.txt
--- a/old.txt
+++ b/new.txt
@@ -1 +1 @@
-old
+new
";
        let files = parse_diff(input).unwrap();
        assert_eq!(files[0].old_path, Some("old.txt".to_string()));
        assert_eq!(files[0].new_path, Some("new.txt".to_string()));
    }

    #[test]
    fn test_parse_blame_multiple_blocks() {
        let input = "abc123def456789012345678901234567890abcd 1 1 1\nauthor Alice\nauthor-mail <a@example.com>\nauthor-time 1700000000\n\tline one\nabc123def456789012345678901234567890abcd 2 2 2\nauthor Alice\nauthor-mail <a@example.com>\nauthor-time 1700000000\n\tline two\n";
        let lines = parse_blame(input).unwrap();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_parse_format_patch_skip_non_from() {
        let input = "some header\nFrom abc123 Mon Sep 17 00:00:00 2001\nFrom: Alice <a@example.com>\nDate: Mon, 1 Jan 2024 00:00:00 +0000\nSubject: [PATCH] Test\n\n---\n\ndiff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let patches = parse_format_patch(input).unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].subject, Some("[PATCH] Test".to_string()));
    }

    #[test]
    fn test_parse_format_patch_empty_diff() {
        let input = "From abc123 Mon Sep 17 00:00:00 2001\nFrom: Alice <a@example.com>\nDate: Mon, 1 Jan 2024 00:00:00 +0000\nSubject: [PATCH] Test\n\n---\n\n";
        let patches = parse_format_patch(input).unwrap();
        assert_eq!(patches.len(), 1);
        assert!(patches[0].diff.is_empty());
    }

    #[test]
    fn test_check_git_version_compat() {
        check_git_version_compat(GitVersion::parse("2.46.0").unwrap());
    }

    #[test]
    fn test_parse_reflog_empty_and_invalid() {
        let r = parse_reflog("\nabc|auth|mail|123|msg|ref\n").unwrap();
        assert_eq!(r.len(), 1);
        let r = parse_reflog("short|line\n").unwrap();
        assert!(r.is_empty());
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        const STATUS_PAIRS: &[(char, char)] = &[
            ('?', '?'),
            ('M', ' '),
            (' ', 'M'),
            ('M', 'M'),
            ('A', ' '),
            (' ', 'D'),
            ('D', ' '),
            ('R', ' '),
            (' ', 'R'),
            ('C', ' '),
            (' ', 'C'),
            ('U', 'U'),
        ];

        proptest! {
            #[test]
            fn prop_parse_diff_shortstat_roundtrip(
                fc in 0..10_000u64,
                ins in 0..10_000u64,
                del in 0..10_000u64,
            ) {
                let fc_str = if fc == 1 { "1 file changed" } else { &format!("{fc} files changed") };
                let ins_str = if ins == 1 { "1 insertion(+)" } else { &format!("{ins} insertions(+)") };
                let del_str = if del == 1 { "1 deletion(-)" } else { &format!("{del} deletions(-)") };
                let input = format!("{fc_str}, {ins_str}, {del_str}");
                let parsed = parse_diff_shortstat(&input).unwrap();
                prop_assert_eq!(parsed.files_changed, fc);
                prop_assert_eq!(parsed.insertions, ins);
                prop_assert_eq!(parsed.deletions, del);
            }

            #[test]
            fn prop_parse_log_roundtrip(
                entries in prop::collection::vec(
                    (
                        "[a-f0-9]{7,40}",
                        "[a-zA-Z0-9_ ./-]{1,50}",
                        "[a-zA-Z0-9_ ./-]{1,30}",
                        0..i64::MAX,
                    ),
                    0..20,
                ),
            ) {
                let mut input = String::new();
                let mut expected = Vec::with_capacity(entries.len());
                for (sha, msg, author, ts) in entries {
                    input.push_str(&format!("{sha}|{msg}|{author}|{ts}\n"));
                    expected.push((sha, msg, author, ts.to_string()));
                }
                let parsed = parse_log(&input).unwrap();
                prop_assert_eq!(parsed.len(), expected.len());
                for (got, (exp_sha, exp_msg, exp_author, exp_ts)) in parsed.iter().zip(expected.iter()) {
                    prop_assert_eq!(&got.sha, exp_sha);
                    prop_assert_eq!(&got.message, exp_msg);
                    prop_assert_eq!(&got.author, exp_author);
                    prop_assert_eq!(&got.timestamp, exp_ts);
                }
            }

            #[test]
            fn prop_parse_log_line_roundtrip(
                sha in "[a-f0-9]{7,40}",
                msg in "[^|]{1,50}",
                author in "[^|]{1,30}",
                ts in 0..i64::MAX,
            ) {
                let line = format!("{sha}|{msg}|{author}|{ts}");
                let entry = parse_log_line(&line).unwrap();
                prop_assert_eq!(entry.sha, sha);
                prop_assert_eq!(entry.message, msg);
                prop_assert_eq!(entry.author, author);
                prop_assert_eq!(entry.timestamp, ts.to_string());
            }

            #[test]
            fn prop_parse_status_counts(
                lines in prop::collection::vec(
                    (
                        prop::sample::select(STATUS_PAIRS),
                        r"[a-zA-Z0-9_ ./-]{1,30}",
                    ),
                    0..30,
                ),
            ) {
                let mut input = String::new();
                let mut expected_staged = 0usize;
                let mut expected_unstaged = 0usize;
                let mut expected_untracked = 0usize;

                for ((idx, wt), path) in &lines {
                    input.push_str(&format!("{idx}{wt} {path}\n"));
                    match (*idx, *wt) {
                        ('?', '?') => expected_untracked += 1,
                        ('!', '!') => {}
                        _ => {
                            if *idx != ' ' {
                                expected_staged += 1;
                            }
                            if *wt != ' ' {
                                expected_unstaged += 1;
                            }
                        }
                    }
                }

                let parsed = parse_status(&input).unwrap();
                prop_assert_eq!(parsed.staged.len(), expected_staged);
                prop_assert_eq!(parsed.unstaged.len(), expected_unstaged);
                prop_assert_eq!(parsed.untracked.len(), expected_untracked);
            }

            #[test]
            fn prop_parse_branches_found(
                names in prop::collection::vec(
                    r"[a-zA-Z0-9_./-]{1,40}",
                    0..20,
                ),
            ) {
                let input = names.join("\n");
                let parsed = parse_branches(&input).unwrap();
                let expected: Vec<String> = names.into_iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
                prop_assert_eq!(parsed, expected);
            }

            #[test]
            fn prop_parse_grep_colons(
                path in r"[a-zA-Z0-9_./-]+",
                line_num in 1..10_000u32,
                text in r"[^\n]{0,100}",
            ) {
                let line = format!("{path}:{line_num}:{text}");
                let results = parse_grep(&line).unwrap();
                prop_assert_eq!(results.len(), 1);
                prop_assert_eq!(results[0].path.clone(), path);
                prop_assert_eq!(results[0].line, line_num);
                prop_assert_eq!(results[0].text.clone(), text);
            }

            #[test]
            fn prop_parse_status_never_panics(s in r"[ MADRCU?!]{2} [^\n]*\n{0,5}") {
                let _ = parse_status(&s);
            }

            #[test]
            fn prop_parse_branches_never_panics(s in r"[a-zA-Z0-9_ /-]*\n{0,10}") {
                let _ = parse_branches(&s);
            }

            #[test]
            fn prop_parse_worktrees_never_panics(s in r"(worktree [^\n]*\n(branch [^\n]*|detached)\n?){0,5}") {
                let _ = parse_worktrees(&s);
            }

            #[test]
            fn prop_parse_merge_tree_never_panics(s in r"[a-z0-9 _:\(\)/\n]{0,200}") {
                let _ = parse_merge_tree(&s);
            }
        }

        #[test]
        fn test_parse_diff_without_git_prefix() {
            let input = r"diff --git old.txt new.txt
--- old.txt
+++ new.txt
@@ -1 +1 @@
-old
+new
";
            let files = parse_diff(input).unwrap();
            assert_eq!(files[0].old_path, Some("old.txt".to_string()));
            assert_eq!(files[0].new_path, Some("new.txt".to_string()));
        }

        #[test]
        fn test_parse_submodules_invalid_prefix() {
            let input = "X1234567890123456789012345678901234567890 path (v1.0)\n";
            let subs = parse_submodules(input).unwrap();
            assert_eq!(subs.len(), 1);
            // When prefix is invalid, after_prefix = line, so sha includes prefix
            assert_eq!(subs[0].sha, "X123456789012345678901234567890123456789");
            assert_eq!(subs[0].path, "0 path");
        }

        #[test]
        fn test_parse_submodules_unclosed_paren() {
            let input = " 1234567890123456789012345678901234567890 path (v1.0\n";
            let subs = parse_submodules(input).unwrap();
            assert_eq!(subs.len(), 1);
            assert_eq!(subs[0].path, "path (v1.0");
            assert_eq!(subs[0].describe, None);
        }

        #[test]
        fn test_parse_blame_sha_header_before_content() {
            let input = "abc123def456789012345678901234567890abcd 1 1 1\nauthor Alice\nabc123def456789012345678901234567890abcd 1 1 1\nauthor Alice\nauthor-mail <a@example.com>\nauthor-time 1700000000\n\tline one\n";
            let lines = parse_blame(input).unwrap();
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0].content, "");
            assert_eq!(lines[1].content, "line one");
        }
    }
}
