use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Parsed git version number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitVersion {
    /// Major version.
    pub major: u32,
    /// Minor version.
    pub minor: u32,
    /// Patch version.
    pub patch: u32,
}

impl fmt::Display for GitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl GitVersion {
    /// Parse a version string like `git version 2.45.1` or `2.45.1`.
    pub fn parse(s: &str) -> Result<Self, crate::error::GitError> {
        let version_part = s.trim().strip_prefix("git version ").unwrap_or(s.trim());
        let mut parts = version_part.split('.');
        let major = parts.next().and_then(|p| p.parse().ok()).ok_or_else(|| {
            crate::error::GitError::Parse(format!("invalid major version in '{s}'"))
        })?;
        let minor = parts.next().and_then(|p| p.parse().ok()).ok_or_else(|| {
            crate::error::GitError::Parse(format!("invalid minor version in '{s}'"))
        })?;
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

/// A validated git object identifier (SHA-1 or SHA-256 prefix).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Oid(String);

impl Oid {
    /// Create a new `Oid` from a string, validating that it contains only hex digits
    /// and is at least 4 characters long.
    pub fn new(s: impl AsRef<str>) -> Result<Self, crate::error::GitError> {
        let s = s.as_ref();
        if s.len() < 4 {
            return Err(crate::error::GitError::Parse(format!(
                "oid too short (minimum 4 chars): {s}"
            )));
        }
        if !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(crate::error::GitError::Parse(format!(
                "oid contains non-hex characters: {s}"
            )));
        }
        Ok(Self(s.to_lowercase()))
    }

    /// Length of the OID string in characters.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the OID string is empty.
    ///
    /// This always returns `false` because the constructor rejects empty
    /// strings (minimum length is 4 characters).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Oid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for Oid {
    type Err = crate::error::GitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Status of a git working tree, parsed from porcelain output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitStatus {
    /// Files with staged changes.
    pub staged: Vec<String>,
    /// Files with unstaged changes.
    pub unstaged: Vec<String>,
    /// Untracked files.
    pub untracked: Vec<String>,
}

/// A single entry from `git log`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitLogEntry {
    /// Full commit SHA.
    pub sha: String,
    /// Short (7-character) commit SHA.
    pub short_sha: String,
    /// Commit message.
    pub message: String,
    /// Author name.
    pub author: String,
    /// Commit timestamp (Unix epoch seconds as string).
    pub timestamp: String,
}

/// A configured git remote.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitRemote {
    /// Remote name (e.g. "origin").
    pub name: String,
    /// Remote URL.
    pub url: String,
}

/// Result of a read-only merge-tree operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitMergeResult {
    /// Whether any conflicts were detected.
    pub has_conflicts: bool,
    /// Files with conflicts (empty if `has_conflicts` is false).
    pub conflict_files: Vec<String>,
    /// The resulting tree OID (if available).
    pub tree_oid: Option<String>,
}

/// A git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitWorktree {
    /// Absolute path to the worktree directory.
    pub path: PathBuf,
    /// Branch tracked by this worktree.
    pub branch: String,
}

/// A git tag.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitTag {
    /// Tag name.
    pub name: String,
    /// Tagged object SHA.
    pub sha: String,
    /// Tag message (empty for lightweight tags).
    pub message: String,
}

/// A single stash entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitStash {
    /// Stash reference (e.g. "stash@{0}").
    pub ref_name: String,
    /// Commit SHA.
    pub sha: String,
    /// Stash message.
    pub message: String,
}

/// Reset mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ResetMode {
    /// Mixed reset (default).
    Mixed,
    /// Soft reset.
    Soft,
    /// Hard reset.
    Hard,
}

/// Result of verifying a GPG signature on a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitVerification {
    /// Whether the signature is valid (or valid but untrusted).
    pub valid: bool,
    /// Signer identity (e.g. "John Doe <john@example.com>").
    pub signer: Option<String>,
    /// GPG key fingerprint.
    pub fingerprint: Option<String>,
    /// Signature status letter from `%G?` (`G`, `U`, `B`, `X`, `Y`, `R`, `E`, `N`).
    pub status: String,
}

/// A git submodule entry from `git submodule status`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitSubmodule {
    /// Current commit SHA recorded in the parent repo.
    pub sha: String,
    /// Path to the submodule directory (relative to repo root).
    pub path: String,
    /// Current branch or tag description (e.g. "(v1.2.3)"), if any.
    pub describe: Option<String>,
    /// Whether the submodule has uncommitted changes (the `+` prefix).
    pub dirty: bool,
    /// Whether the submodule is uninitialized (the `-` prefix).
    pub uninitialized: bool,
}

/// A single match from `git grep`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitGrepResult {
    /// File path containing the match.
    pub path: String,
    /// 1-based line number.
    pub line: u32,
    /// Matching line text.
    pub text: String,
}

/// A single line inside a diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DiffLineKind {
    /// Unchanged context line.
    Context,
    /// Line that was removed.
    Deletion,
    /// Line that was added.
    Insertion,
    /// "No newline at end of file" marker.
    NoNewline,
}

/// A single line inside a diff hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiffLine {
    /// Kind of diff line.
    pub kind: DiffLineKind,
    /// Line content (without the leading ` ` / `-` / `+` marker).
    pub content: String,
}

/// A hunk inside a file diff.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiffHunk {
    /// Starting line number in the old file.
    pub old_start: usize,
    /// Number of lines in the old file.
    pub old_lines: usize,
    /// Starting line number in the new file.
    pub new_start: usize,
    /// Number of lines in the new file.
    pub new_lines: usize,
    /// Section header (e.g. "fn main() {").
    pub section: String,
    /// Individual lines.
    pub lines: Vec<DiffLine>,
}

/// A single file diff.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FileDiff {
    /// Old file path (None for new files).
    pub old_path: Option<String>,
    /// New file path (None for deleted files).
    pub new_path: Option<String>,
    /// Whether the file is binary.
    pub is_binary: bool,
    /// Whether the file mode changed.
    pub mode_changed: bool,
    /// Old mode (e.g. "100644").
    pub old_mode: Option<String>,
    /// New mode (e.g. "100755").
    pub new_mode: Option<String>,
    /// Hunks for text files.
    pub hunks: Vec<DiffHunk>,
}

/// A single line from `git blame --line-porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BlameLine {
    /// Commit SHA.
    pub commit: String,
    /// Author name.
    pub author: String,
    /// Author email.
    pub author_mail: String,
    /// Author timestamp (Unix epoch seconds).
    pub author_time: String,
    /// Line number in the final file.
    pub line_no: usize,
    /// Line content.
    pub content: String,
}

/// A patch generated by `git format-patch`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Patch {
    /// Commit SHA (from the `From` header).
    pub commit: Option<String>,
    /// Patch subject line.
    pub subject: Option<String>,
    /// Author (from the `From:` header).
    pub from: Option<String>,
    /// Date (from the `Date:` header).
    pub date: Option<String>,
    /// Structured diff contained in the patch.
    pub diff: Vec<FileDiff>,
}

/// Result of applying a patch.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ApplyReport {
    /// Files that were successfully modified.
    pub files_changed: Vec<String>,
}

/// A single entry from `git reflog`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ReflogEntry {
    /// Commit SHA.
    pub commit: String,
    /// Author name.
    pub author: String,
    /// Author email.
    pub author_mail: String,
    /// Timestamp (Unix epoch seconds).
    pub timestamp: String,
    /// Reflog subject (e.g. "commit: message").
    pub subject: String,
    /// Reflog designator (e.g. "HEAD@{0}").
    pub designator: String,
}

/// A git hook in `.git/hooks/`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Hook {
    /// Hook name (e.g. "pre-commit").
    pub name: String,
    /// Absolute path to the hook script.
    pub path: PathBuf,
    /// Whether the hook is executable (always true on Windows).
    pub active: bool,
}

/// Output from running a git hook.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HookOutput {
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Process exit code.
    pub exit_code: i32,
}

/// State of an in-progress `git bisect`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BisectState {
    /// The commit currently being tested (from `BISECT_HEAD`).
    pub current: Option<String>,
}

/// Result of a completed `git bisect run`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BisectResult {
    /// The first bad commit SHA if found.
    pub found: Option<String>,
    /// Number of commits remaining to test.
    pub remaining: usize,
    /// Log messages from bisect.
    pub log: Vec<String>,
}

/// A git note entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitNote {
    /// Object the note is attached to.
    pub object: String,
    /// Note commit SHA.
    pub commit: String,
}

/// A single git attribute result.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitAttr {
    /// File path the attribute applies to.
    pub path: String,
    /// Attribute name.
    pub attr: String,
    /// Attribute value (e.g. "set", "unset", "unspecified", or a custom value).
    pub value: String,
}

/// A tree entry for `git mktree`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TreeEntry {
    /// File mode (e.g. "100644", "100755", "040000", "160000", "120000").
    pub mode: String,
    /// File path.
    pub path: String,
    /// Object OID.
    pub oid: Oid,
}

/// An index entry from `git ls-files --stage`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IndexEntry {
    /// File path.
    pub path: String,
    /// Object OID.
    pub oid: Oid,
    /// File mode.
    pub mode: u32,
}

/// Kind of a git object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ObjectKind {
    /// Blob object (file content).
    Blob,
    /// Tree object (directory listing).
    Tree,
    /// Commit object.
    Commit,
    /// Tag object (annotated tag).
    Tag,
}

impl std::str::FromStr for ObjectKind {
    type Err = crate::error::GitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "blob" => Ok(Self::Blob),
            "tree" => Ok(Self::Tree),
            "commit" => Ok(Self::Commit),
            "tag" => Ok(Self::Tag),
            _ => Err(crate::error::GitError::Parse(format!(
                "unknown object kind: {s}"
            ))),
        }
    }
}

impl fmt::Display for ObjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blob => write!(f, "blob"),
            Self::Tree => write!(f, "tree"),
            Self::Commit => write!(f, "commit"),
            Self::Tag => write!(f, "tag"),
        }
    }
}

/// Raw content of a git object from `git cat-file`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ObjectContent {
    /// Object OID.
    pub oid: Oid,
    /// Object kind.
    pub kind: ObjectKind,
    /// Size in bytes.
    pub size: usize,
    /// Raw content bytes.
    pub data: Vec<u8>,
}

/// A parsed commit object.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitCommit {
    /// Tree OID.
    pub tree: Oid,
    /// Parent commit OIDs.
    pub parents: Vec<Oid>,
    /// Author line.
    pub author: String,
    /// Committer line.
    pub committer: String,
    /// Commit message.
    pub message: String,
}

/// Options for `Repository::push`.
#[derive(Debug, Clone, Default)]
pub struct PushOptions<'a> {
    /// Remote to push to.
    pub remote: &'a str,
    /// Branch to push.
    pub branch: &'a str,
    /// Use `--force`.
    pub force: bool,
    /// Use `--force-with-lease`.
    pub force_with_lease: bool,
    /// Use `--set-upstream`.
    pub set_upstream: bool,
}

/// Options for `Repository::commit`.
#[derive(Debug, Clone)]
pub struct CommitOptions<'a> {
    /// Commit message.
    pub message: &'a str,
    /// Paths to commit. Empty means commit all changes (`-a`).
    pub paths: &'a [&'a std::path::Path],
    /// Skip pre-commit and commit-msg hooks.
    pub no_verify: bool,
    /// Amend the previous commit.
    pub amend: bool,
    /// Add a Signed-off-by trailer.
    pub signoff: bool,
}

impl<'a> CommitOptions<'a> {
    /// Create a new `CommitOptions` with the given message and defaults.
    pub fn new(message: &'a str) -> Self {
        Self {
            message,
            paths: &[],
            no_verify: false,
            amend: false,
            signoff: false,
        }
    }
}

/// Options for `Repository::rebase`.
#[derive(Debug, Clone, Default)]
pub struct RebaseOptions<'a> {
    /// Branch to rebase onto.
    pub branch: &'a str,
    /// Run an interactive rebase.
    pub interactive: bool,
    /// Automatically squash fixup commits.
    pub autosquash: bool,
    /// Rebase onto a different starting point.
    pub onto: Option<&'a str>,
}

/// Options for `Repository::merge`.
#[derive(Debug, Clone, Default)]
pub struct MergeOptions<'a> {
    /// Branch to merge into the current branch.
    pub branch: &'a str,
    /// Do not open an editor for the merge message.
    pub no_edit: bool,
    /// Create a merge commit even when the merge resolves as a fast-forward.
    pub no_ff: bool,
    /// Squash and merge.
    pub squash: bool,
}

/// Options for `Repository::fetch`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FetchOptions<'a> {
    /// Remote to fetch from.
    pub remote: &'a str,
    /// Prune stale remote-tracking branches.
    pub prune: bool,
    /// Fetch all tags.
    pub tags: bool,
    /// Limit fetching to the specified number of commits.
    pub depth: Option<usize>,
    /// Partial clone filter (e.g. `blob:none`, `tree:0`).
    pub filter: Option<&'a str>,
}

/// Options for `Repository::clone`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CloneOptions<'a> {
    /// URL to clone from.
    pub url: &'a str,
    /// Local path to clone into.
    pub path: &'a Path,
    /// Limit cloning to the specified number of commits.
    pub depth: Option<usize>,
    /// Clone only the specified branch.
    pub branch: Option<&'a str>,
    /// Partial clone filter (e.g. `blob:none`, `tree:0`).
    pub filter: Option<&'a str>,
    /// Perform a bare clone.
    pub bare: bool,
}

impl Default for CloneOptions<'_> {
    fn default() -> Self {
        Self {
            url: "",
            path: Path::new("."),
            depth: None,
            branch: None,
            filter: None,
            bare: false,
        }
    }
}

/// A file tracked by Git LFS.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GitLfsFile {
    /// LFS object ID (OID).
    pub oid: String,
    /// File path (relative to repo root).
    pub path: String,
    /// File size in bytes, if known.
    pub size: Option<u64>,
}

/// Options for `Repository::cherry_pick`.
#[derive(Debug, Clone, Default)]
pub struct CherryPickOptions<'a> {
    /// Commits to cherry-pick.
    pub commits: &'a [&'a str],
    /// Apply changes without making a commit.
    pub no_commit: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_version_parse_and_display() {
        let v = GitVersion::parse("git version 2.45.1").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 45);
        assert_eq!(v.patch, 1);
        assert_eq!(v.to_string(), "2.45.1");

        let v = GitVersion::parse("3.0").unwrap();
        assert_eq!(v.to_string(), "3.0.0");
    }

    #[test]
    fn test_git_version_parse_errors() {
        let err = GitVersion::parse("not a version").unwrap_err();
        assert!(matches!(err, crate::error::GitError::Parse(ref s) if s.contains("major")));
        let err = GitVersion::parse("2.not_a_minor").unwrap_err();
        assert!(matches!(err, crate::error::GitError::Parse(ref s) if s.contains("minor")));
    }

    #[test]
    fn test_oid_new_and_display() {
        let oid = Oid::new("abc123").unwrap();
        assert_eq!(oid.to_string(), "abc123");
        assert_eq!(oid.len(), 6);
        assert!(!oid.is_empty());
        assert_eq!(oid.as_ref(), "abc123");
        assert_eq!("abc123".parse::<Oid>().unwrap(), oid);
    }

    #[test]
    fn test_oid_new_errors() {
        let err = Oid::new("ab").unwrap_err();
        assert!(matches!(err, crate::error::GitError::Parse(ref s) if s.contains("too short")));
        let err = Oid::new("xyz!").unwrap_err();
        assert!(matches!(err, crate::error::GitError::Parse(ref s) if s.contains("non-hex")));
    }

    #[test]
    fn test_object_kind_from_str_and_display() {
        assert_eq!("blob".parse::<ObjectKind>().unwrap(), ObjectKind::Blob);
        assert_eq!("tree".parse::<ObjectKind>().unwrap(), ObjectKind::Tree);
        assert_eq!("commit".parse::<ObjectKind>().unwrap(), ObjectKind::Commit);
        assert_eq!("tag".parse::<ObjectKind>().unwrap(), ObjectKind::Tag);
        assert_eq!(ObjectKind::Blob.to_string(), "blob");
        assert_eq!(ObjectKind::Tree.to_string(), "tree");
        assert_eq!(ObjectKind::Commit.to_string(), "commit");
        assert_eq!(ObjectKind::Tag.to_string(), "tag");
        let err = "unknown".parse::<ObjectKind>().unwrap_err();
        assert!(
            matches!(err, crate::error::GitError::Parse(ref s) if s.contains("unknown object kind"))
        );
    }

    #[test]
    fn test_commit_options_new() {
        let opts = CommitOptions::new("msg");
        assert_eq!(opts.message, "msg");
        assert!(opts.paths.is_empty());
        assert!(!opts.no_verify);
    }

    #[test]
    fn test_clone_options_default() {
        let opts = CloneOptions::default();
        assert_eq!(opts.url, "");
        assert_eq!(opts.path, Path::new("."));
        assert!(!opts.bare);
    }
}
