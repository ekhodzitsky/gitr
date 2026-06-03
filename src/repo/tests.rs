use super::*;
use std::process::Command;
use tempfile::TempDir;

fn git_available() -> bool {
    which::which("git").is_ok()
}

fn run_git(dir: &std::path::Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(dir).env("LC_ALL", "C");
    let out = cmd.output().unwrap();
    if !out.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn temp_repo_dir() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    run_git(dir, &["init"]);
    run_git(dir, &["config", "user.email", "test@test.com"]);
    run_git(dir, &["config", "user.name", "Test"]);
    std::fs::write(dir.join("init.txt"), "init").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "init"]);
    run_git(dir, &["branch", "-m", "main"]);
    tmp
}

#[tokio::test]
async fn test_open_valid_repo() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    assert!(repo.root().exists());
}

#[tokio::test]
async fn test_open_not_a_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let err = Repository::open(tmp.path()).await.unwrap_err();
    assert!(matches!(err, GitError::NotARepo(_)));
}

#[tokio::test]
async fn test_ensure_clean_clean() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.ensure_clean().await.unwrap();
}

#[tokio::test]
async fn test_ensure_clean_dirty() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "dirty").unwrap();
    let err = repo.ensure_clean().await.unwrap_err();
    assert!(matches!(err, GitError::Dirty(_)));
}

#[tokio::test]
async fn test_current_branch() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let branch = repo.current_branch().await.unwrap();
    assert_eq!(branch, "main");
}

#[tokio::test]
async fn test_head_commit() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let sha = repo.head_commit().await.unwrap();
    assert_eq!(sha.len(), 7);
}

#[tokio::test]
async fn test_changed_files() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "modified").unwrap();
    std::fs::write(tmp.path().join("new.txt"), "hello").unwrap();
    let files = repo.changed_files().await.unwrap();
    assert!(files.contains(&"init.txt".to_string()));
    assert!(files.contains(&"new.txt".to_string()));
}

#[tokio::test]
async fn test_changed_files_no_duplicates() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    // Stage init.txt, then modify it again -> porcelain shows "MM init.txt"
    std::fs::write(tmp.path().join("init.txt"), "v1").unwrap();
    run_git(tmp.path(), &["add", "init.txt"]);
    std::fs::write(tmp.path().join("init.txt"), "v2").unwrap();
    let files = repo.changed_files().await.unwrap();
    assert_eq!(files.iter().filter(|f| *f == "init.txt").count(), 1);
}

#[tokio::test]
async fn test_worktree_add_and_remove() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("wt-branch", None).await.unwrap();
    let wt_path = tmp.path().join("wt");
    let wt = repo.worktree_add(&wt_path, "wt-branch").await.unwrap();
    assert_eq!(wt.branch, "wt-branch");

    let list = repo.worktree_list().await.unwrap();
    assert!(list.iter().any(|w| w.branch == "wt-branch"));

    repo.worktree_remove(&wt_path, false).await.unwrap();
    let list = repo.worktree_list().await.unwrap();
    assert!(!list.iter().any(|w| w.branch == "wt-branch"));
}

#[tokio::test]
async fn test_worktree_add_existing_path() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("wt-branch", None).await.unwrap();
    let wt_path = tmp.path().join("wt");
    repo.worktree_add(&wt_path, "wt-branch").await.unwrap();
    let err = repo.worktree_add(&wt_path, "wt-branch").await.unwrap_err();
    assert!(matches!(err, GitError::WorktreeExists(_)));
}

#[tokio::test]
async fn test_branch_create_delete() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("feature-x", None).await.unwrap();
    assert!(repo.branch_exists("feature-x").await.unwrap());

    repo.branch_delete("feature-x", false).await.unwrap();
    assert!(!repo.branch_exists("feature-x").await.unwrap());
}

#[tokio::test]
async fn test_merge_tree_clean() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("feature", None).await.unwrap();
    repo.checkout("feature").await.unwrap();
    std::fs::write(tmp.path().join("feature.txt"), "feat").unwrap();
    run_git(tmp.path(), &["add", "."]);
    run_git(tmp.path(), &["commit", "-m", "feature commit"]);
    repo.checkout("main").await.unwrap();

    let result = repo.merge_tree("main", "feature").await.unwrap();
    assert!(!result.has_conflicts);
}

#[tokio::test]
async fn test_merge_tree_conflicts() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("conflict.txt"), "base").unwrap();
    run_git(tmp.path(), &["add", "."]);
    run_git(tmp.path(), &["commit", "-m", "base conflict"]);

    repo.branch_create("a", None).await.unwrap();
    repo.branch_create("b", None).await.unwrap();

    repo.checkout("a").await.unwrap();
    std::fs::write(tmp.path().join("conflict.txt"), "a").unwrap();
    run_git(tmp.path(), &["add", "."]);
    run_git(tmp.path(), &["commit", "-m", "a"]);

    repo.checkout("b").await.unwrap();
    std::fs::write(tmp.path().join("conflict.txt"), "b").unwrap();
    run_git(tmp.path(), &["add", "."]);
    run_git(tmp.path(), &["commit", "-m", "b"]);

    let result = repo.merge_tree("a", "b").await.unwrap();
    assert!(result.has_conflicts);
    assert!(!result.conflict_files.is_empty());
}

#[tokio::test]
async fn test_commit() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let old_sha = repo.head_commit().await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "new data").unwrap();
    let new_sha = repo.commit("test commit", &[] as &[&std::path::Path]).await.unwrap();
    assert_ne!(old_sha, new_sha);
}

#[tokio::test]
async fn test_stash_pop() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "stashed").unwrap();
    repo.stash(Some("my stash")).await.unwrap();
    repo.ensure_clean().await.unwrap();

    repo.stash_pop().await.unwrap();
    let files = repo.changed_files().await.unwrap();
    assert!(files.iter().any(|f| f.contains("init.txt")));
}

#[tokio::test]
async fn test_diff() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "hello diff").unwrap();
    let diff = repo.diff().await.unwrap();
    assert!(diff.contains("diff --git"));
}

#[tokio::test]
async fn test_diff_files() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "a").unwrap();
    std::fs::write(tmp.path().join("other.txt"), "b").unwrap();
    let diff = repo.diff_files(&[tmp.path().join("init.txt")]).await.unwrap();
    assert!(diff.contains("init.txt"));
    assert!(!diff.contains("other.txt"));
}

#[tokio::test]
async fn test_untracked_files() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("untracked.txt"), "u").unwrap();
    let files = repo.untracked_files().await.unwrap();
    assert!(files.contains(&"untracked.txt".to_string()));
}

#[tokio::test]
async fn test_fetch_and_remote_url() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let url = repo.remote_url("origin").await.unwrap();
    assert!(url.is_none());
}

#[tokio::test]
async fn test_commit_with_paths() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "modified").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
    let sha = repo
        .commit("commit init only", &[std::path::Path::new("init.txt")])
        .await
        .unwrap();
    let files = repo.changed_files().await.unwrap();
    assert!(!files.contains(&"init.txt".to_string()));
    assert!(files.contains(&"b.txt".to_string()));
    assert!(!sha.is_empty());
}

#[tokio::test]
async fn test_checkout_branch_not_found() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.checkout("nonexistent-branch-12345").await.unwrap_err();
    assert!(matches!(err, GitError::BranchNotFound(_)));
}
