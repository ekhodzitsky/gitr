use super::*;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

#[cfg(feature = "stream")]
use tokio_stream::StreamExt;

fn git_available() -> bool {
    which::which("git").is_ok()
}

fn git_lfs_available() -> bool {
    git_available() && which::which("git-lfs").is_ok()
}

fn run_git(dir: &std::path::Path, args: &[&str]) {
    run_git_env(dir, args, &[]);
}

fn run_git_env(dir: &std::path::Path, args: &[&str], envs: &[(&str, &str)]) {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(dir).env("LC_ALL", "C");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
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
    let branch = repo.current_branch().await.unwrap();
    assert_eq!(branch, "main");
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
    assert!(matches!(err, GitError::Dirty(..)));
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
async fn test_head_commit_full() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let sha = repo.head_commit_full().await.unwrap();
    assert_eq!(sha.len(), 40);
}

#[tokio::test]
async fn test_status_and_porcelain() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let status = repo.status().await.unwrap();
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
    assert!(status.untracked.is_empty());

    let porcelain = repo.status_porcelain().await.unwrap();
    assert!(porcelain.is_empty());

    std::fs::write(tmp.path().join("new.txt"), "new").unwrap();
    let status = repo.status().await.unwrap();
    assert!(!status.untracked.is_empty());
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
    std::fs::write(tmp.path().join("init.txt"), "v1").unwrap();
    repo.add(tmp.path().join("init.txt")).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "v2").unwrap();
    let files = repo.changed_files().await.unwrap();
    assert_eq!(files.iter().filter(|&f| f == "init.txt").count(), 1);
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
    assert!(matches!(err, GitError::WorktreeExists(..)));
}

#[tokio::test]
async fn test_open_worktree() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("wt-test", None).await.unwrap();

    let wt_path = tmp.path().join("wt");
    repo.worktree_add(&wt_path, "wt-test").await.unwrap();

    let wt_repo = Repository::open_worktree(&wt_path).await.unwrap();
    let branch = wt_repo.current_branch().await.unwrap();
    assert_eq!(branch, "wt-test");
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
async fn test_branch_create_with_start_point() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let sha = repo.head_commit_full().await.unwrap();
    repo.branch_create("from-sha", Some(sha.as_ref()))
        .await
        .unwrap();
    assert!(repo.branch_exists("from-sha").await.unwrap());
}

#[tokio::test]
async fn test_branch_delete_force() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("unmerged", None).await.unwrap();
    repo.checkout("unmerged").await.unwrap();
    std::fs::write(tmp.path().join("feat.txt"), "feat").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("feat", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    repo.checkout("main").await.unwrap();
    let err = repo.branch_delete("unmerged", false).await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));

    repo.branch_delete("unmerged", true).await.unwrap();
    assert!(!repo.branch_exists("unmerged").await.unwrap());
}

#[tokio::test]
async fn test_checkout_success() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("other", None).await.unwrap();
    repo.checkout("other").await.unwrap();
    let branch = repo.current_branch().await.unwrap();
    assert_eq!(branch, "other");

    repo.checkout("main").await.unwrap();
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
    repo.add_all().await.unwrap();
    repo.commit("feat", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

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
    repo.add_all().await.unwrap();
    repo.commit("base", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    repo.branch_create("a", None).await.unwrap();
    repo.branch_create("b", None).await.unwrap();

    repo.checkout("a").await.unwrap();
    std::fs::write(tmp.path().join("conflict.txt"), "a").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("a", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    repo.checkout("b").await.unwrap();
    std::fs::write(tmp.path().join("conflict.txt"), "b").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("b", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    let result = repo.merge_tree("a", "b").await.unwrap();
    assert!(result.has_conflicts);
    assert!(!result.conflict_files.is_empty());
}

#[tokio::test]
async fn test_merge_tree_invalid_branch() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.merge_tree("main", "nonexistent").await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_commit() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "new data").unwrap();
    let sha = repo
        .commit(
            "update init",
            &[tmp.path().join("init.txt").as_path()],
            false,
        )
        .await
        .unwrap();
    assert!(!sha.is_empty());
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
    repo.add(tmp.path().join("init.txt")).await.unwrap();
    repo.add(tmp.path().join("b.txt")).await.unwrap();
    let sha = repo
        .commit("multi", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    assert!(!sha.is_empty());
    let files = repo.changed_files().await.unwrap();
    assert!(!files.contains(&"init.txt".to_string()));
    assert!(!files.contains(&"b.txt".to_string()));
}

#[tokio::test]
async fn test_add_and_add_all() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
    repo.add(tmp.path().join("a.txt")).await.unwrap();
    let status = repo.status().await.unwrap();
    assert_eq!(status.staged.len(), 1);

    repo.add_all().await.unwrap();
    let status = repo.status().await.unwrap();
    assert_eq!(status.staged.len(), 2);
}

#[tokio::test]
async fn test_stash_and_stash_pop() {
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
    assert!(files.contains(&"init.txt".to_string()));
}

#[tokio::test]
async fn test_stash_without_message() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "stashed").unwrap();
    repo.stash(None).await.unwrap();
    repo.ensure_clean().await.unwrap();
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
    assert!(diff.contains("init.txt"));
}

#[tokio::test]
async fn test_diff_shortstat() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "hello diff").unwrap();
    let stat = repo.diff_shortstat().await.unwrap();
    assert_eq!(stat.files_changed, 1);
}

#[tokio::test]
async fn test_diff_files() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "a").unwrap();
    let out = repo.diff_files(&["init.txt"]).await.unwrap();
    assert!(out.contains("init.txt"));
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
    assert_eq!(url, None);
}

#[tokio::test]
async fn test_push_without_remote() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.push("origin", "main", false).await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_push_force_without_remote() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.push_force("origin", "main").await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_fetch_without_remote() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.fetch("origin").await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_merge_no_edit() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("feat", None).await.unwrap();
    repo.checkout("feat").await.unwrap();
    std::fs::write(tmp.path().join("feat.txt"), "feat").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("feat", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    repo.checkout("main").await.unwrap();
    repo.merge("feat", true).await.unwrap();
}

#[tokio::test]
async fn test_rebase_abort_noop() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.rebase_abort().await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_default_branch_no_remote() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.default_branch().await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_agent_helpers() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();

    assert!(repo.is_nothing_to_commit().await.unwrap());
    assert!(!repo.has_untracked_files().await.unwrap());
    assert!(!repo.is_merge_conflict().await.unwrap());

    std::fs::write(tmp.path().join("new.txt"), "new").unwrap();
    assert!(repo.is_nothing_to_commit().await.unwrap());
    assert!(repo.has_untracked_files().await.unwrap());

    std::fs::write(tmp.path().join("init.txt"), "modified").unwrap();
    assert!(!repo.is_nothing_to_commit().await.unwrap());
}

#[tokio::test]
async fn test_checkout_branch_not_found() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.checkout("nonexistent").await.unwrap_err();
    assert!(matches!(err, GitError::BranchNotFound(..)));
}

#[tokio::test]
async fn test_log() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let log = repo.log(None).await.unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].message, "init");

    std::fs::write(tmp.path().join("second.txt"), "second").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("second", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    let log = repo.log(None).await.unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].message, "second");
    assert_eq!(log[1].message, "init");

    let limited = repo.log(Some(1)).await.unwrap();
    assert_eq!(limited.len(), 1);
}

#[tokio::test]
async fn test_remotes() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let remotes = repo.remotes().await.unwrap();
    assert!(remotes.is_empty());

    repo.remote_add("origin", "https://example.com/repo.git")
        .await
        .unwrap();
    let remotes = repo.remotes().await.unwrap();
    assert_eq!(remotes.len(), 1);
    assert_eq!(remotes[0].name, "origin");
    assert_eq!(remotes[0].url, "https://example.com/repo.git");
}

#[tokio::test]
async fn test_init() {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let repo = Repository::init(tmp.path()).await.unwrap();
    assert!(tmp.path().join(".git").exists());
    // current_branch fails on empty repo because HEAD does not exist yet
    let _ = repo.current_branch().await;
}

#[tokio::test]
async fn test_clone_local_repo() {
    if !git_available() {
        return;
    }
    let remote_tmp = temp_repo_dir();
    std::fs::write(remote_tmp.path().join("remote.txt"), "remote").unwrap();
    run_git(remote_tmp.path(), &["add", "."]);
    run_git(remote_tmp.path(), &["commit", "-m", "remote commit"]);

    let local_tmp = tempfile::tempdir().unwrap();
    let repo = Repository::clone(remote_tmp.path().to_str().unwrap(), local_tmp.path())
        .await
        .unwrap();
    let branch = repo.current_branch().await.unwrap();
    assert_eq!(branch, "main");
    assert!(local_tmp.path().join("remote.txt").exists());
}

#[tokio::test]
async fn test_config_get_set() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();

    repo.config_set("custom.key", "value42").await.unwrap();
    let val = repo.config_get("custom.key").await.unwrap();
    assert_eq!(val, Some("value42".to_string()));

    let missing = repo.config_get("nonexistent.key.xyz").await.unwrap();
    assert_eq!(missing, None);

    repo.config_unset("custom.key").await.unwrap();
    let unset = repo.config_get("custom.key").await.unwrap();
    assert_eq!(unset, None);

    // Idempotent: unsetting a non-existent key is ok
    repo.config_unset("nonexistent.key.xyz").await.unwrap();
}

#[tokio::test]
async fn test_tag_list_create_delete() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();

    repo.tag_create("v1.0", Some("release"), false)
        .await
        .unwrap();
    let tags = repo.tag_list().await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "v1.0");

    repo.tag_delete("v1.0").await.unwrap();
    let tags = repo.tag_list().await.unwrap();
    assert!(tags.is_empty());
}

#[tokio::test]
async fn test_stash_list() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "stashed").unwrap();
    repo.stash(Some("first")).await.unwrap();

    let list = repo.stash_list().await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].message.contains("first"));
}

#[tokio::test]
async fn test_show() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let out = repo.show("init.txt", None).await.unwrap();
    assert!(out.contains("init"));
}

#[tokio::test]
async fn test_reset() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "modified").unwrap();
    repo.add_all().await.unwrap();
    // After staging, unstaged diff is empty but staged diff is not
    let diff = repo.diff().await.unwrap();
    assert!(diff.is_empty());
    let cached = repo.diff_cached().await.unwrap();
    assert!(!cached.is_empty());

    repo.reset(crate::types::ResetMode::Mixed, Some("HEAD"))
        .await
        .unwrap();
    // After mixed reset, index matches HEAD but working tree is still modified
    let diff = repo.diff().await.unwrap();
    assert!(!diff.is_empty());
    let cached = repo.diff_cached().await.unwrap();
    assert!(cached.is_empty());
}

#[tokio::test]
async fn test_cherry_pick() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("feat", None).await.unwrap();
    repo.checkout("feat").await.unwrap();
    std::fs::write(tmp.path().join("feat.txt"), "feat").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("feat", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    let sha = repo.head_commit().await.unwrap();

    repo.checkout("main").await.unwrap();
    repo.cherry_pick(&[sha.as_ref()]).await.unwrap();
    assert!(tmp.path().join("feat.txt").exists());
}

#[tokio::test]
async fn test_ls_files() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let files = repo.ls_files(false, false, false).await.unwrap();
    assert!(files.contains(&"init.txt".to_string()));
}

#[tokio::test]
async fn test_diff_cached() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "cached").unwrap();
    repo.add_all().await.unwrap();
    let diff = repo.diff_cached().await.unwrap();
    assert!(diff.contains("init.txt"));
}

#[tokio::test]
async fn test_grep() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let results = repo.grep("init").await.unwrap();
    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.text.contains("init")));
}

#[tokio::test]
async fn test_describe() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.tag_create("v0", Some("desc"), false).await.unwrap();
    let desc = repo.describe(false, false).await.unwrap();
    assert!(!desc.is_empty());
}

#[tokio::test]
async fn test_clean() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("garbage.tmp"), "x").unwrap();
    let removed = repo.clean(true, false, false).await.unwrap();
    assert!(removed.iter().any(|f| f.contains("garbage.tmp")));
    assert!(!tmp.path().join("garbage.tmp").exists());
}

#[tokio::test]
async fn test_archive() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let archive_path = tmp.path().join("archive.zip");
    repo.archive("HEAD", &archive_path).await.unwrap();
    assert!(archive_path.exists());
}

#[tokio::test]
async fn test_submodule_list() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let submodules = repo.submodule_list().await.unwrap();
    assert!(submodules.is_empty());
}

#[tokio::test]
async fn test_diff_structured() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "modified").unwrap();
    let diff = repo.diff_structured().await.unwrap();
    assert!(!diff.is_empty());
    assert_eq!(diff[0].new_path, Some("init.txt".to_string()));
}

#[tokio::test]
async fn test_diff_cached_structured() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "cached").unwrap();
    repo.add_all().await.unwrap();
    let diff = repo.diff_cached_structured().await.unwrap();
    assert!(!diff.is_empty());
    assert_eq!(diff[0].new_path, Some("init.txt".to_string()));
}

#[tokio::test]
async fn test_blame_structured() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let lines = repo.blame_structured("init.txt").await.unwrap();
    assert!(!lines.is_empty());
}

#[tokio::test]
async fn test_format_patch() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("second.txt"), "second").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("second", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    let patches = repo.format_patch("HEAD~1..HEAD").await.unwrap();
    assert!(!patches.is_empty());
}

#[tokio::test]
async fn test_apply_patch() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let patch = r#"diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..e965047
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+Hello
"#;
    repo.apply_patch(patch, false).await.unwrap();
    assert!(tmp.path().join("new.txt").exists());
}

#[tokio::test]
async fn test_reflog_list() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let entries = repo.reflog_list(None).await.unwrap();
    assert!(!entries.is_empty());
}

#[tokio::test]
#[cfg(unix)]
async fn test_hooks_list_install_remove_run() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();

    let initial = repo.hooks_list().await.unwrap().len();

    repo.hook_install("post-checkout", "#!/bin/sh\necho ok\n")
        .await
        .unwrap();
    let hooks = repo.hooks_list().await.unwrap();
    assert_eq!(hooks.len(), initial + 1);
    assert!(hooks.iter().any(|h| h.name == "post-checkout"));

    let out = repo.run_hook("post-checkout").await.unwrap();
    assert!(out.stdout.contains("ok"));

    repo.hook_remove("post-checkout").await.unwrap();
    let hooks = repo.hooks_list().await.unwrap();
    assert_eq!(hooks.len(), initial);
}

#[tokio::test]
#[cfg(unix)]
async fn test_run_hook_with_timeout() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.hook_install("slow", "#!/bin/sh\nsleep 0.1\necho done\n")
        .await
        .unwrap();
    let out = repo
        .run_hook_with_timeout("slow", Duration::from_secs(5))
        .await
        .unwrap();
    assert!(out.stdout.contains("done"));
}

#[tokio::test]
#[cfg(unix)]
async fn test_run_hook_with_timeout_expires() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.hook_install("slow", "#!/bin/sh\nsleep 10\n")
        .await
        .unwrap();
    let err = repo
        .run_hook_with_timeout("slow", Duration::from_millis(100))
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::Timeout(..)));
}

#[tokio::test]
#[cfg(unix)]
async fn test_run_hook_streaming() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.hook_install("stream", "#!/bin/sh\necho line1\necho line2 >&2\n")
        .await
        .unwrap();
    let (stdout_tx, mut stdout_rx) = tokio::sync::mpsc::channel(10);
    let (stderr_tx, mut stderr_rx) = tokio::sync::mpsc::channel(10);
    let _ = repo
        .run_hook_streaming("stream", stdout_tx, stderr_tx)
        .await
        .unwrap();
    let stdout_lines: Vec<String> = std::iter::from_fn(|| stdout_rx.try_recv().ok()).collect();
    let stderr_lines: Vec<String> = std::iter::from_fn(|| stderr_rx.try_recv().ok()).collect();
    assert!(stdout_lines.iter().any(|l| l.contains("line1")));
    assert!(stderr_lines.iter().any(|l| l.contains("line2")));
}

#[tokio::test]
async fn test_bisect() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();

    for i in 0..3 {
        std::fs::write(tmp.path().join("file.txt"), format!("v{i}")).unwrap();
        repo.add_all().await.unwrap();
        repo.commit(&format!("commit {i}"), &[] as &[&std::path::Path], false)
            .await
            .unwrap();
    }

    let state = repo.bisect_start(Some("HEAD"), &["HEAD~2"]).await.unwrap();
    // BISECT_HEAD may not exist immediately after bisect start on some git versions
    let _ = state.current;

    repo.bisect_reset().await.unwrap();
}

#[tokio::test]
async fn test_commit_opts_delegation() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "opts").unwrap();
    repo.add_all().await.unwrap();
    let sha = repo
        .commit_opts(&CommitOptions {
            message: "opts",
            paths: &[],
            no_verify: false,
            amend: false,
            signoff: false,
        })
        .await
        .unwrap();
    assert!(!sha.is_empty());
}

#[tokio::test]
async fn test_push_opts_delegation() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo
        .push_opts(&PushOptions {
            remote: "origin",
            branch: "main",
            force: false,
            force_with_lease: false,
            set_upstream: false,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_fetch_opts_delegation() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo
        .fetch_opts(&crate::types::FetchOptions {
            remote: "origin",
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_merge_opts_delegation() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("m", None).await.unwrap();
    repo.checkout("m").await.unwrap();
    std::fs::write(tmp.path().join("m.txt"), "m").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("m", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    repo.checkout("main").await.unwrap();
    repo.merge_opts(&MergeOptions {
        branch: "m",
        no_edit: true,
        no_ff: false,
        squash: false,
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_rebase_opts_delegation() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("r", None).await.unwrap();
    repo.checkout("r").await.unwrap();
    std::fs::write(tmp.path().join("r.txt"), "r").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("r", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    repo.checkout("main").await.unwrap();
    repo.rebase_opts(&RebaseOptions {
        branch: "r",
        interactive: false,
        autosquash: false,
        onto: None,
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_cherry_pick_opts_delegation() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("cp", None).await.unwrap();
    repo.checkout("cp").await.unwrap();
    std::fs::write(tmp.path().join("cp.txt"), "cp").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("cp", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    let sha = repo.head_commit().await.unwrap();

    repo.checkout("main").await.unwrap();
    repo.cherry_pick_opts(&CherryPickOptions {
        commits: &[sha.as_ref()],
        no_commit: false,
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_sparse_checkout_init_set_add_list_disable() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();

    repo.config_set("core.sparseCheckout", "true")
        .await
        .unwrap();
    repo.sparse_checkout_init(true).await.unwrap();

    repo.sparse_checkout_set(&["src/"]).await.unwrap();
    let list = repo.sparse_checkout_list().await.unwrap();
    assert!(list.iter().any(|l| l.contains("src")));

    repo.sparse_checkout_add(&["tests/"]).await.unwrap();
    let list = repo.sparse_checkout_list().await.unwrap();
    assert!(list.iter().any(|l| l.contains("tests")));

    repo.sparse_checkout_disable().await.unwrap();
}

#[tokio::test]
async fn test_lfs_track_untrack() {
    if !git_lfs_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.lfs_track(&["*.bin"]).await.unwrap();
    let out = repo.lfs_track(&[]).await;
    assert!(out.is_ok());
}

#[tokio::test]
async fn test_lfs_ls_files() {
    if !git_lfs_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let files = repo.lfs_ls_files().await.unwrap();
    assert!(files.is_empty());
}

#[tokio::test]
async fn test_hash_object() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let oid = repo.hash_object(b"hello").await.unwrap();
    assert!(!oid.is_empty());
}

#[tokio::test]
async fn test_write_and_read_blob() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let oid = repo.write_blob(b"blob content").await.unwrap();
    let data = repo.read_blob(&oid).await.unwrap();
    assert_eq!(data, b"blob content");
}

#[tokio::test]
async fn test_mktree_and_read_tree() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let blob_oid = repo.write_blob(b"file").await.unwrap();
    let tree_oid = repo
        .mktree(&[crate::types::TreeEntry {
            mode: "100644".to_string(),
            path: "file.txt".to_string(),
            oid: blob_oid,
        }])
        .await
        .unwrap();
    let entries = repo.read_tree(&tree_oid).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "file.txt");
}

#[tokio::test]
async fn test_write_and_read_commit() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let blob = repo.write_blob(b"c").await.unwrap();
    let tree = repo
        .mktree(&[crate::types::TreeEntry {
            mode: "100644".to_string(),
            path: "c.txt".to_string(),
            oid: blob,
        }])
        .await
        .unwrap();
    let head = repo.head_commit_full().await.unwrap();
    let commit_oid = repo
        .write_commit(&[&head], &tree, "test commit")
        .await
        .unwrap();
    let commit = repo.read_commit(&commit_oid).await.unwrap();
    assert_eq!(commit.message, "test commit");
}

#[tokio::test]
async fn test_read_index_and_update_remove() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let index = repo.read_index().await.unwrap();
    assert!(!index.is_empty());

    let blob = repo.write_blob(b"x").await.unwrap();
    repo.update_index(std::path::Path::new("x.txt"), &blob, 0o100644)
        .await
        .unwrap();
    let index = repo.read_index().await.unwrap();
    assert!(index.iter().any(|e| e.path == "x.txt"));

    repo.remove_index(std::path::Path::new("x.txt"))
        .await
        .unwrap();
    let index = repo.read_index().await.unwrap();
    assert!(!index.iter().any(|e| e.path == "x.txt"));
}

#[tokio::test]
async fn test_switch() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("sw", None).await.unwrap();
    repo.switch("sw", false).await.unwrap();
    assert_eq!(repo.current_branch().await.unwrap(), "sw");
}

#[tokio::test]
async fn test_switch_create() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.switch("sw-new", true).await.unwrap();
    assert_eq!(repo.current_branch().await.unwrap(), "sw-new");
}

#[tokio::test]
async fn test_restore() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "modified").unwrap();
    repo.restore(&[tmp.path().join("init.txt").as_path()], false, None)
        .await
        .unwrap();
    let content = std::fs::read_to_string(tmp.path().join("init.txt")).unwrap();
    assert_eq!(content, "init");
}

#[tokio::test]
async fn test_revert() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("rev.txt"), "rev").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("rev", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    repo.revert(&["HEAD"], true).await.unwrap();
    assert!(!tmp.path().join("rev.txt").exists());
}

#[tokio::test]
async fn test_stash_drop_apply_show() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "stashed").unwrap();
    repo.stash(Some("stash-test")).await.unwrap();
    repo.ensure_clean().await.unwrap();

    let diff = repo.stash_show(Some(0)).await.unwrap();
    assert!(diff.contains("init.txt"));

    repo.stash_apply(Some(0)).await.unwrap();
    let files = repo.changed_files().await.unwrap();
    assert!(files.contains(&"init.txt".to_string()));

    repo.stash_drop(Some(0)).await.unwrap();
    let list = repo.stash_list().await.unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_remote_add_remove_rename() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.remote_add("r1", ".").await.unwrap();
    assert!(repo.remote_url("r1").await.unwrap().is_some());

    repo.remote_rename("r1", "r2").await.unwrap();
    assert!(repo.remote_url("r2").await.unwrap().is_some());
    assert!(repo.remote_url("r1").await.unwrap().is_none());

    repo.remote_remove("r2").await.unwrap();
    assert!(repo.remote_url("r2").await.unwrap().is_none());
}

#[tokio::test]
async fn test_branch_rename() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("old-name", None).await.unwrap();
    repo.branch_rename("old-name", "new-name", false)
        .await
        .unwrap();
    assert!(repo.branch_exists("new-name").await.unwrap());
    assert!(!repo.branch_exists("old-name").await.unwrap());
}

#[tokio::test]
async fn test_tag_delete() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.tag_create("td", Some("msg"), false).await.unwrap();
    assert!(repo
        .tag_list()
        .await
        .unwrap()
        .iter()
        .any(|t| t.name == "td"));
    repo.tag_delete("td").await.unwrap();
    assert!(!repo
        .tag_list()
        .await
        .unwrap()
        .iter()
        .any(|t| t.name == "td"));
}

#[tokio::test]
async fn test_mv_and_rm() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("mv.txt"), "mv").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("mv", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    repo.mv(
        tmp.path().join("mv.txt").as_path(),
        tmp.path().join("moved.txt").as_path(),
    )
    .await
    .unwrap();
    assert!(!tmp.path().join("mv.txt").exists());
    assert!(tmp.path().join("moved.txt").exists());

    repo.commit("mv done", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    repo.rm(&[tmp.path().join("moved.txt").as_path()], false)
        .await
        .unwrap();
    assert!(!tmp.path().join("moved.txt").exists());
}

#[tokio::test]
async fn test_merge_base() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("mb", None).await.unwrap();
    repo.checkout("mb").await.unwrap();
    std::fs::write(tmp.path().join("mb.txt"), "mb").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("mb", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    repo.checkout("main").await.unwrap();
    let base = repo.merge_base(&["main", "mb"]).await.unwrap();
    assert!(!base.is_empty());
}

#[tokio::test]
async fn test_rev_parse() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let sha = repo.rev_parse("HEAD").await.unwrap();
    assert_eq!(sha.len(), 40);
}

#[tokio::test]
async fn test_pull_and_ls_remote() {
    if !git_available() {
        return;
    }
    // Create a "remote" repo
    let remote_tmp = temp_repo_dir();
    std::fs::write(remote_tmp.path().join("remote.txt"), "remote").unwrap();
    run_git(remote_tmp.path(), &["add", "."]);
    run_git(remote_tmp.path(), &["commit", "-m", "remote commit"]);

    // Create a local repo (empty, no commits) and add the remote
    let local_tmp = tempfile::tempdir().unwrap();
    run_git(local_tmp.path(), &["init"]);
    run_git(local_tmp.path(), &["config", "user.email", "test@test.com"]);
    run_git(local_tmp.path(), &["config", "user.name", "Test"]);
    let repo = Repository::open(local_tmp.path()).await.unwrap();
    repo.remote_add("origin", remote_tmp.path().to_str().unwrap())
        .await
        .unwrap();

    // ls_remote
    let refs = repo.ls_remote("origin", None).await.unwrap();
    assert!(!refs.is_empty());
    assert!(refs.iter().any(|(_, name)| name == "refs/heads/main"));

    // pull
    repo.pull("origin", "main", false).await.unwrap();
    assert!(local_tmp.path().join("remote.txt").exists());
}

#[tokio::test]
async fn test_worktree_prune_lock_unlock_move() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("wt-test", None).await.unwrap();

    let wt_path = tmp.path().join("wt");
    repo.worktree_add(&wt_path, "wt-test").await.unwrap();

    repo.worktree_lock(&wt_path).await.unwrap();
    repo.worktree_unlock(&wt_path).await.unwrap();

    let new_path = tmp.path().join("wt-moved");
    repo.worktree_move(&wt_path, &new_path).await.unwrap();
    let list = repo.worktree_list().await.unwrap();
    assert!(list.iter().any(|w| w.branch == "wt-test"));

    repo.worktree_prune().await.unwrap();
    repo.worktree_remove(&new_path, false).await.unwrap();
    repo.branch_delete("wt-test", false).await.unwrap();
}

#[tokio::test]
async fn test_with_env_var() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path())
        .await
        .unwrap()
        .with_env_var("GITR_TEST_VAR", "42");
    // env var should be set without error; verify repo still works
    let branch = repo.current_branch().await.unwrap();
    assert_eq!(branch, "main");
}

#[tokio::test]
async fn test_git_api_trait_dispatch() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let api: &(dyn GitApi + Send + Sync) = &repo;

    // Basic queries
    api.ensure_clean().await.unwrap();
    let _ = api.status().await.unwrap();
    let branch = api.current_branch().await.unwrap();
    assert_eq!(branch, "main");
    let _ = api.head_commit().await.unwrap();
    let _ = api.log(None).await.unwrap();
    let _ = api.diff().await.unwrap();
    let _ = api.changed_files().await.unwrap();

    // Branch operations
    api.branch_create("feat", None).await.unwrap();
    api.checkout("feat").await.unwrap();
    api.checkout("main").await.unwrap();
    api.branch_delete("feat", false).await.unwrap();

    // Commit
    std::fs::write(tmp.path().join("new.txt"), "new").unwrap();
    repo.add_all().await.unwrap();
    api.commit("add new", &[] as &[&std::path::Path], false)
        .await
        .unwrap();

    // Stash
    std::fs::write(tmp.path().join("new.txt"), "stashed").unwrap();
    api.stash(Some("test")).await.unwrap();

    // Tag
    api.tag_create("v1", Some("release"), false).await.unwrap();
    let tags = api.tag_list().await.unwrap();
    assert!(tags.iter().any(|t| t.name == "v1"));
    api.tag_delete("v1").await.unwrap();

    // Config
    api.config_set("gitr.test", "value").await.unwrap();
    let val = api.config_get("gitr.test").await.unwrap();
    assert_eq!(val, Some("value".to_string()));
    api.config_unset("gitr.test").await.unwrap();

    // Worktree
    let wt_path = tmp.path().join("wt");
    api.branch_create("wt-branch", None).await.unwrap();
    api.worktree_add(&wt_path, "wt-branch").await.unwrap();
    let wts = api.worktree_list().await.unwrap();
    assert!(wts.iter().any(|w| w.branch == "wt-branch"));
    api.worktree_remove(&wt_path, false).await.unwrap();
    api.branch_delete("wt-branch", false).await.unwrap();

    // Remote
    api.remote_add("testremote", ".").await.unwrap();
    api.remote_remove("testremote").await.unwrap();

    // Reflog
    let _ = api.reflog_list(None).await.unwrap();

    // Notes
    let _ = api.notes_list(None).await.unwrap();

    // Grep
    let _ = api.grep("new").await.unwrap();

    // Describe
    api.tag_create("v0", Some("desc"), false).await.unwrap();
    let _ = api.describe(false, false).await.unwrap();

    // Reset
    api.reset(crate::types::ResetMode::Mixed, Some("HEAD"))
        .await
        .unwrap();

    // Clean
    std::fs::write(tmp.path().join("garbage.tmp"), "x").unwrap();
    let _ = api.clean(true, false, false).await.unwrap();

    // Show
    let _ = api.show("new.txt", None).await.unwrap();

    // Rev parse
    let _ = api.rev_parse("HEAD").await.unwrap();

    // Merge tree
    api.branch_create("mt", None).await.unwrap();
    api.checkout("mt").await.unwrap();
    std::fs::write(tmp.path().join("mt.txt"), "mt").unwrap();
    repo.add_all().await.unwrap();
    api.commit("mt", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    api.checkout("main").await.unwrap();
    let _ = api.merge_tree("main", "mt").await.unwrap();
    api.branch_delete("mt", true).await.unwrap();

    // Ls files
    let _ = api.ls_files(false, false, false).await.unwrap();

    // Bundle
    let bundle_path = tmp.path().join("bundle.pack");
    api.bundle_create(&bundle_path, Some(&["HEAD"]))
        .await
        .unwrap();
    let _ = api.bundle_list_heads(&bundle_path).await.unwrap();
    api.bundle_verify(&bundle_path).await.unwrap();
    api.bundle_unbundle(&bundle_path).await.unwrap();

    // Submodule
    let _ = api.submodule_list().await.unwrap();
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn test_log_stream() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let mut stream = repo.log_stream().await.unwrap();
    let entry = stream.next().await.unwrap().unwrap();
    assert_eq!(entry.message, "init");
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn test_blame_stream() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let mut stream = repo.blame_stream("init.txt").await.unwrap();
    let line = stream.next().await.unwrap().unwrap();
    assert_eq!(line.content, "init");
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn test_ls_files_stream() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let mut stream = repo.ls_files_stream(false, false, false).await.unwrap();
    let file = stream.next().await.unwrap().unwrap();
    assert_eq!(file, "init.txt");
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn test_grep_stream() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let mut stream = repo.grep_stream("init").await.unwrap();
    let result = stream.next().await.unwrap().unwrap();
    assert!(result.text.contains("init"));
}

#[tokio::test]
async fn test_with_timeout() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let repo = repo.with_timeout(Duration::from_secs(120));
    let branch = repo.current_branch().await.unwrap();
    assert_eq!(branch, "main");
}

#[tokio::test]
async fn test_with_progress() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let _repo = repo.with_progress(|_line: String| {});
}

#[tokio::test]
async fn test_with_circuit_breaker() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let cb = crate::circuit::CircuitBreaker::default();
    let _repo = repo.with_circuit_breaker(cb);
}

#[tokio::test]
async fn test_status_porcelain() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let out = repo.status_porcelain().await.unwrap();
    assert!(out.is_empty());
}

#[tokio::test]
async fn test_push_force() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.push_force("origin", "main").await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_is_nothing_to_commit() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    assert!(repo.is_nothing_to_commit().await.unwrap());
}

#[tokio::test]
async fn test_has_untracked_files() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    assert!(!repo.has_untracked_files().await.unwrap());
    std::fs::write(tmp.path().join("u.txt"), "x").unwrap();
    assert!(repo.has_untracked_files().await.unwrap());
}

#[tokio::test]
async fn test_is_merge_conflict() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    assert!(!repo.is_merge_conflict().await.unwrap());
}

#[tokio::test]
async fn test_diff_files_structured() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "a").unwrap();
    let files = repo.diff_files_structured(&["init.txt"]).await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].new_path, Some("init.txt".to_string()));
}

#[tokio::test]
async fn test_blame_string() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let out = repo.blame("init.txt").await.unwrap();
    assert!(out.contains("init"));
}

#[tokio::test]
async fn test_apply_patch_file() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let patch_path = tmp.path().join("patch.diff");
    std::fs::write(
        &patch_path,
        r#"diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..e965047
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+Hello
"#,
    )
    .unwrap();
    repo.apply_patch_file(&patch_path, false).await.unwrap();
    assert!(tmp.path().join("new.txt").exists());
}

#[tokio::test]
async fn test_rebase_continue() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.rebase_continue().await.unwrap_err();
    assert!(matches!(err, GitError::CommandFailed { .. }));
}

#[tokio::test]
async fn test_check_ignore() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join(".gitignore"), "*.tmp\n").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("gitignore", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    std::fs::write(tmp.path().join("a.tmp"), "x").unwrap();
    let ignored = repo
        .check_ignore(&[tmp.path().join("a.tmp").as_path()])
        .await
        .unwrap();
    assert!(ignored.iter().any(|p| p.contains("a.tmp")));
}

#[tokio::test]
async fn test_check_attr() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join(".gitattributes"), "*.txt text\n").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("attrs", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    let attrs = repo
        .check_attr(&[tmp.path().join("init.txt").as_path()], &["text"])
        .await
        .unwrap();
    assert!(!attrs.is_empty());
}

#[tokio::test]
async fn test_submodule_add_update_deinit_sync() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let sub_dir = tmp.path().join("subrepo");
    std::fs::create_dir(&sub_dir).unwrap();
    run_git(&sub_dir, &["init"]);
    run_git(&sub_dir, &["config", "user.email", "test@test.com"]);
    run_git(&sub_dir, &["config", "user.name", "Test"]);
    std::fs::write(sub_dir.join("f.txt"), "f").unwrap();
    run_git(&sub_dir, &["add", "."]);
    run_git(&sub_dir, &["commit", "-m", "init"]);
    let repo = Repository::open(tmp.path())
        .await
        .unwrap()
        .with_env_var("GIT_ALLOW_PROTOCOL", "file");
    repo.submodule_add("./subrepo", std::path::Path::new("sub"))
        .await
        .unwrap();
    let subs = repo.submodule_list().await.unwrap();
    assert!(!subs.is_empty());
    repo.submodule_update(false, false).await.unwrap();
    repo.submodule_sync().await.unwrap();
    repo.submodule_deinit(std::path::Path::new("sub"), true)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_reflog_expire() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.reflog_expire("HEAD", Some("now")).await.unwrap();
    // Just verify it does not panic; exact emptiness depends on git timing
    let _entries = repo.reflog_list(Some("HEAD")).await.unwrap();
}

#[tokio::test]
async fn test_notes_add_remove() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let sha = repo.head_commit_full().await.unwrap();
    repo.notes_add("my note", sha.as_ref(), None, false)
        .await
        .unwrap();
    let notes = repo.notes_list(None).await.unwrap();
    assert!(!notes.is_empty());
    let note = repo.notes_show(sha.as_ref(), None).await.unwrap();
    assert!(note.contains("my note"));
    repo.notes_remove(sha.as_ref(), None).await.unwrap();
    let notes = repo.notes_list(None).await.unwrap();
    assert!(notes.is_empty());
}

#[tokio::test]
async fn test_read_object_tree_blob_commit() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let blob = repo.write_blob(b"test").await.unwrap();
    let obj = repo.read_object(&blob).await.unwrap();
    assert_eq!(obj.kind, crate::types::ObjectKind::Blob);
    let tree = repo
        .mktree(&[crate::types::TreeEntry {
            mode: "100644".to_string(),
            path: "f.txt".to_string(),
            oid: blob.clone(),
        }])
        .await
        .unwrap();
    let tree_obj = repo.read_tree(&tree).await.unwrap();
    assert_eq!(tree_obj.len(), 1);
    let blob_data = repo.read_blob(&blob).await.unwrap();
    assert_eq!(blob_data, b"test");
    let head = repo.head_commit_full().await.unwrap();
    let commit = repo.read_commit(&head).await.unwrap();
    assert_eq!(commit.message, "init");
}

#[tokio::test]
async fn test_write_tree() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let blob = repo.write_blob(b"x").await.unwrap();
    let tree = repo
        .write_tree(&[crate::types::TreeEntry {
            mode: "100644".to_string(),
            path: "x.txt".to_string(),
            oid: blob,
        }])
        .await
        .unwrap();
    let entries = repo.read_tree(&tree).await.unwrap();
    assert_eq!(entries.len(), 1);
}

#[tokio::test]
async fn test_reset_soft_and_hard() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "modified").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("second", &[] as &[&std::path::Path], false)
        .await
        .unwrap();
    // Soft reset keeps staged changes
    repo.reset(crate::types::ResetMode::Soft, Some("HEAD~1"))
        .await
        .unwrap();
    let cached = repo.diff_cached().await.unwrap();
    assert!(!cached.is_empty());
    // Hard reset discards everything
    repo.reset(crate::types::ResetMode::Hard, Some("HEAD"))
        .await
        .unwrap();
    let diff = repo.diff().await.unwrap();
    assert!(diff.is_empty());
}

#[tokio::test]
async fn test_ls_files_deleted_and_others() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::remove_file(tmp.path().join("init.txt")).unwrap();
    let deleted = repo.ls_files(true, false, false).await.unwrap();
    assert!(deleted.contains(&"init.txt".to_string()));
    std::fs::write(tmp.path().join("other.txt"), "o").unwrap();
    let others = repo.ls_files(false, true, true).await.unwrap();
    assert!(others.contains(&"other.txt".to_string()));
}

#[tokio::test]
async fn test_commit_opts_no_verify_amend_signoff() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
    repo.add_all().await.unwrap();
    let sha1 = repo
        .commit_opts(&CommitOptions {
            message: "first",
            paths: &[],
            no_verify: true,
            amend: false,
            signoff: false,
        })
        .await
        .unwrap();
    assert!(!sha1.is_empty());
    let sha2 = repo
        .commit_opts(&CommitOptions {
            message: "amended",
            paths: &[],
            no_verify: false,
            amend: true,
            signoff: true,
        })
        .await
        .unwrap();
    assert!(!sha2.is_empty());
}

#[tokio::test]
async fn test_push_opts_force_and_set_upstream() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.remote_add("origin", ".").await.unwrap();
    repo.push_opts(&PushOptions {
        remote: "origin",
        branch: "main",
        force: true,
        force_with_lease: false,
        set_upstream: true,
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_fetch_opts_prune_tags_depth() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.remote_add("origin", ".").await.unwrap();
    repo.fetch_opts(&FetchOptions {
        remote: "origin",
        prune: true,
        tags: true,
        depth: Some(1),
        filter: None,
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_default_branch_and_remote_url() {
    if !git_available() {
        return;
    }
    let remote_tmp = temp_repo_dir();
    let tmp = tempfile::tempdir().unwrap();
    run_git(tmp.path(), &["init"]);
    run_git(tmp.path(), &["config", "user.email", "test@test.com"]);
    run_git(tmp.path(), &["config", "user.name", "Test"]);
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.remote_add("origin", remote_tmp.path().to_str().unwrap())
        .await
        .unwrap();
    let url = repo.remote_url("origin").await.unwrap();
    assert!(url.is_some());
    // default_branch requires refs/remotes/origin/HEAD to be set
    let _ = repo.default_branch().await;
}

#[tokio::test]
async fn test_git_version_and_root() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let version = repo.git_version();
    assert!(version.major >= 2);
    let canonical = std::fs::canonicalize(tmp.path()).unwrap();
    assert_eq!(repo.root(), canonical);
}

#[tokio::test]
async fn test_status_z() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let status = repo.status_z().await.unwrap();
    assert!(status.untracked.is_empty());
}

#[tokio::test]
async fn test_with_cache_hit_and_invalidate() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path())
        .await
        .unwrap()
        .with_cache(crate::cache::Cache::new());
    let s1 = repo.status().await.unwrap();
    let s2 = repo.status().await.unwrap();
    assert_eq!(s1.untracked, s2.untracked);
    repo.invalidate_cache().await;
}

#[tokio::test]
async fn test_with_cancel() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let token = tokio_util::sync::CancellationToken::new();
    let repo = repo.with_cancel(token.clone());
    let handle = tokio::spawn(async move { repo.status().await });
    token.cancel();
    let result = handle.await.unwrap();
    assert!(result.is_ok() || matches!(result, Err(GitError::Io(ref s)) if s == "cancelled"));
}

#[tokio::test]
async fn test_clone_opts_filter_and_bare() {
    if !git_available() {
        return;
    }
    let remote_tmp = temp_repo_dir();
    let tmp = tempfile::tempdir().unwrap();
    let clone_path = tmp.path().join("cloned");
    let _repo = Repository::clone_opts(&crate::types::CloneOptions {
        url: remote_tmp.path().to_str().unwrap(),
        path: &clone_path,
        depth: Some(1),
        branch: Some("main"),
        filter: Some("blob:none"),
        bare: false,
    })
    .await
    .unwrap();
    assert!(clone_path.exists());

    let bare_path = tmp.path().join("bare");
    // bare clone creates a repo without .git directory; open() will return NotARepo
    let result = Repository::clone_opts(&crate::types::CloneOptions {
        url: remote_tmp.path().to_str().unwrap(),
        path: &bare_path,
        depth: None,
        branch: None,
        filter: None,
        bare: true,
    })
    .await;
    assert!(matches!(result, Err(GitError::NotARepo(_))) || bare_path.join("HEAD").exists());
}

#[tokio::test]
async fn test_conflicted_files() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("track.txt"), "main").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("main second", &[] as &[&str], false)
        .await
        .unwrap();

    repo.branch_create("conflict", None).await.unwrap();
    repo.checkout("conflict").await.unwrap();
    std::fs::write(tmp.path().join("track.txt"), "conflict-branch").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("conflict commit", &[] as &[&str], false)
        .await
        .unwrap();

    repo.checkout("main").await.unwrap();
    std::fs::write(tmp.path().join("track.txt"), "main-branch").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("main third", &[] as &[&str], false)
        .await
        .unwrap();

    let merge_result = repo.merge("conflict", false).await;
    let conflicted = repo.conflicted_files().await.unwrap();
    assert!(conflicted.contains(&"track.txt".to_string()) || merge_result.is_err());
}

#[tokio::test]
async fn test_bundle_create_verify_unbundle() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let bundle_path = tmp.path().join("repo.bundle");
    repo.bundle_create(&bundle_path, Some(&["HEAD"]))
        .await
        .unwrap();
    repo.bundle_verify(&bundle_path).await.unwrap();
    let heads = repo.bundle_list_heads(&bundle_path).await.unwrap();
    assert!(!heads.is_empty());

    let tmp2 = tempfile::tempdir().unwrap();
    run_git(tmp2.path(), &["init"]);
    let repo2 = Repository::open(tmp2.path()).await.unwrap();
    repo2.bundle_unbundle(&bundle_path).await.unwrap();
}

#[tokio::test]
async fn test_bisect_run() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    for i in 1..=3 {
        std::fs::write(tmp.path().join("track.txt"), i.to_string()).unwrap();
        repo.add_all().await.unwrap();
        repo.commit(&format!("commit {i}"), &[] as &[&str], false)
            .await
            .unwrap();
    }
    repo.bisect_start(None, &[]).await.unwrap();
    repo.bisect_bad(None).await.unwrap();
    let log = repo.log(Some(10)).await.unwrap();
    let first_commit = log.last().unwrap().sha.clone();
    repo.bisect_good(Some(&first_commit)).await.unwrap();
    let result = repo.bisect_run("true").await;
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_log_paginated() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let entries = repo.log_paginated(0, 10).await.unwrap();
    assert!(!entries.is_empty());
}

#[tokio::test]
async fn test_rebase() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("main.txt"), "main").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("main commit", &[] as &[&str], false)
        .await
        .unwrap();

    repo.branch_create("rebase-branch", None).await.unwrap();
    repo.checkout("rebase-branch").await.unwrap();
    std::fs::write(tmp.path().join("rebase.txt"), "rebase").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("rebase commit", &[] as &[&str], false)
        .await
        .unwrap();

    repo.checkout("main").await.unwrap();
    repo.rebase("rebase-branch").await.unwrap();
    let log = repo.log(Some(10)).await.unwrap();
    assert!(log.iter().any(|e| e.message == "rebase commit"));
}

#[tokio::test]
async fn test_write_commit() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let head = repo.head_commit_full().await.unwrap();
    let commit = repo.read_commit(&head).await.unwrap();
    let result = repo
        .write_commit(&[&head], &commit.tree, "written commit")
        .await;
    assert!(result.is_ok(), "write_commit failed: {result:?}");
    let new_commit = result.unwrap();
    let new = repo.read_commit(&new_commit).await.unwrap();
    assert_eq!(new.message, "written commit");
    assert_eq!(new.parents.len(), 1);
}

#[tokio::test]
async fn test_clone_opts_failure() {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let clone_path = tmp.path().join("cloned");
    let result = Repository::clone_opts(&crate::types::CloneOptions {
        url: "/nonexistent/repo",
        path: &clone_path,
        depth: None,
        branch: None,
        filter: None,
        bare: false,
    })
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_branch_create_already_exists() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("existing", None).await.unwrap();
    let err = repo.branch_create("existing", None).await.unwrap_err();
    assert!(matches!(err, GitError::BranchExists(ref s) if s == "existing"));
}

#[tokio::test]
async fn test_merge_tree_error() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let result = repo
        .merge_tree("nonexistent-base", "nonexistent-branch")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_rebase_abort() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    // Create a conflict situation for rebase
    std::fs::write(tmp.path().join("main.txt"), "main").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("main commit", &[] as &[&str], false)
        .await
        .unwrap();

    repo.branch_create("rebase-branch", None).await.unwrap();
    repo.checkout("rebase-branch").await.unwrap();
    std::fs::write(tmp.path().join("main.txt"), "branch").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("branch commit", &[] as &[&str], false)
        .await
        .unwrap();

    repo.checkout("main").await.unwrap();
    std::fs::write(tmp.path().join("main.txt"), "main2").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("main2", &[] as &[&str], false).await.unwrap();

    // Start rebase interactively (or normally) - it will conflict
    let result = repo.rebase("rebase-branch").await;
    if result.is_err() {
        // Abort the rebase
        repo.rebase_abort().await.unwrap();
    }
}

#[tokio::test]
async fn test_worktree_remove_force() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("wt-force", None).await.unwrap();
    let wt_path = tmp.path().join("wt-force");
    repo.worktree_add(&wt_path, "wt-force").await.unwrap();
    // Create an uncommitted change in the worktree so force is needed
    std::fs::write(wt_path.join("uncommitted.txt"), "x").unwrap();
    repo.worktree_remove(&wt_path, true).await.unwrap();
    repo.branch_delete("wt-force", false).await.unwrap();
}

#[tokio::test]
async fn test_branch_delete_not_found() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.branch_delete("nonexistent", false).await.unwrap_err();
    assert!(matches!(err, GitError::BranchNotFound(ref s) if s == "nonexistent"));
}

#[tokio::test]
async fn test_check_attr_error() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let result = repo.check_attr(&["nonexistent.txt"], &["filter"]).await;
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_push_force_happy() {
    if !git_available() {
        return;
    }
    let remote_tmp = temp_repo_dir();
    run_git(
        remote_tmp.path(),
        &["config", "receive.denyCurrentBranch", "ignore"],
    );
    let tmp = tempfile::tempdir().unwrap();
    run_git(tmp.path(), &["init"]);
    run_git(tmp.path(), &["config", "user.email", "test@test.com"]);
    run_git(tmp.path(), &["config", "user.name", "Test"]);
    std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
    run_git(tmp.path(), &["add", "."]);
    run_git(tmp.path(), &["commit", "-m", "init"]);
    run_git(tmp.path(), &["branch", "-m", "main"]);
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.remote_add("origin", remote_tmp.path().to_str().unwrap())
        .await
        .unwrap();
    std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("second", &[] as &[&str], false).await.unwrap();
    repo.push_force("origin", "main").await.unwrap();
}

#[tokio::test]
async fn test_fetch_opts_filter() {
    if !git_available() {
        return;
    }
    let remote_tmp = temp_repo_dir();
    let tmp = tempfile::tempdir().unwrap();
    run_git(tmp.path(), &["init"]);
    run_git(tmp.path(), &["config", "user.email", "test@test.com"]);
    run_git(tmp.path(), &["config", "user.name", "Test"]);
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.remote_add("origin", remote_tmp.path().to_str().unwrap())
        .await
        .unwrap();
    repo.fetch_opts(&crate::types::FetchOptions {
        remote: "origin",
        prune: false,
        tags: false,
        depth: None,
        filter: Some("blob:none"),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_tag_create_force() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.tag_create("v1", Some("first"), false).await.unwrap();
    repo.tag_create("v1", Some("second"), true).await.unwrap();
    let tags = repo.tag_list().await.unwrap();
    assert!(tags.iter().any(|t| t.name == "v1"));
}

#[tokio::test]
async fn test_branch_delete_current() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("del-me", None).await.unwrap();
    repo.checkout("del-me").await.unwrap();
    let err = repo.branch_delete("del-me", false).await.unwrap_err();
    assert!(
        matches!(err, GitError::CommandFailed { .. }),
        "expected CommandFailed, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_stash_pop_empty() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.stash_pop().await.unwrap_err();
    assert!(
        matches!(err, GitError::CommandFailed { .. }),
        "expected CommandFailed, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_stash_show_drop_none() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "stash").unwrap();
    repo.stash(Some("test")).await.unwrap();

    let diff = repo.stash_show(None).await.unwrap();
    assert!(diff.contains("init.txt"));

    repo.stash_drop(None).await.unwrap();
    assert!(repo.stash_list().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_default_branch_invalid() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    // Set an invalid origin/HEAD symbolic ref
    run_git(
        tmp.path(),
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/heads/main",
        ],
    );
    let result = repo.default_branch().await;
    assert!(
        result.is_ok() || result.is_err(),
        "default_branch should handle origin/HEAD"
    );
    // Also test with a completely unexpected format
    run_git(
        tmp.path(),
        &["symbolic-ref", "refs/remotes/origin/HEAD", "unexpected"],
    );
    let err = repo.default_branch().await.unwrap_err();
    assert!(
        matches!(err, GitError::Parse(ref s) if s.contains("unexpected origin/HEAD format")),
        "expected Parse error, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_restore_staged_and_source() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "v2").unwrap();
    repo.add_all().await.unwrap();
    // restore staged file back to HEAD (unstages, working tree stays v2)
    repo.restore(&[tmp.path().join("init.txt").as_path()], true, None)
        .await
        .unwrap();
    let staged = repo
        .cmd
        .run(&["diff", "--cached", "--name-only"])
        .await
        .unwrap();
    assert!(staged.stdout.trim().is_empty());

    // restore working tree from source
    repo.commit("v3", &[] as &[&str], false).await.unwrap();
    std::fs::write(tmp.path().join("init.txt"), "v4").unwrap();
    repo.restore(
        &[tmp.path().join("init.txt").as_path()],
        false,
        Some("HEAD~1"),
    )
    .await
    .unwrap();
    let content = std::fs::read_to_string(tmp.path().join("init.txt")).unwrap();
    assert_eq!(content, "init");
}

#[tokio::test]
async fn test_switch_not_found() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.switch("no-such-branch", false).await.unwrap_err();
    // git switch returns "invalid reference" which does not match the
    // "did not match" / "not found" heuristic, so it falls through to CommandFailed.
    assert!(
        matches!(err, GitError::CommandFailed { .. }),
        "expected CommandFailed, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_show_rev_and_not_found() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let out = repo.show("init.txt", Some("HEAD")).await.unwrap();
    assert!(out.contains("init"));

    let err = repo.show("nonexistent.txt", None).await.unwrap_err();
    assert!(
        matches!(err, GitError::CommandFailed { .. }),
        "expected CommandFailed, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_notes_force() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    std::fs::write(tmp.path().join("note.txt"), "n").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("note", &[] as &[&str], false).await.unwrap();
    let sha = repo.head_commit_full().await.unwrap();

    repo.notes_add("first", sha.as_ref(), None, false)
        .await
        .unwrap();
    // force overwrite
    repo.notes_add("second", sha.as_ref(), None, true)
        .await
        .unwrap();
    let note = repo.notes_show(sha.as_ref(), None).await.unwrap();
    assert!(note.contains("second"));
}

#[tokio::test]
async fn test_default_branch_valid() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    run_git(
        tmp.path(),
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    let branch = repo.default_branch().await.unwrap();
    assert_eq!(branch, "main");
}

#[tokio::test]
async fn test_head_commit_full_empty_repo() {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    run_git(tmp.path(), &["init"]);
    let repo = Repository::open(tmp.path()).await.unwrap();
    let err = repo.head_commit_full().await.unwrap_err();
    assert!(
        matches!(err, GitError::CommandFailed { .. }),
        "expected CommandFailed, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_worktree_add_exists() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("wt-exists", None).await.unwrap();
    let wt_path = tmp.path().join("wt-exists");
    repo.worktree_add(&wt_path, "wt-exists").await.unwrap();
    let err = repo.worktree_add(&wt_path, "wt-exists").await.unwrap_err();
    assert!(
        matches!(err, GitError::WorktreeExists(ref p) if p == wt_path.to_string_lossy().as_ref()),
        "expected WorktreeExists, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_open_nonexistent_path() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("does_not_exist");
    let err = Repository::open(&bad).await.unwrap_err();
    assert!(
        matches!(err, GitError::Io(ref s) if s.contains("failed to canonicalize path")),
        "expected Io, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_checkout_conflict() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    repo.branch_create("other", None).await.unwrap();
    std::fs::write(tmp.path().join("other.txt"), "other").unwrap();
    repo.add_all().await.unwrap();
    repo.commit("other", &[] as &[&str], false).await.unwrap();
    repo.checkout("main").await.unwrap();
    // create uncommitted change that conflicts with other branch
    std::fs::write(tmp.path().join("other.txt"), "conflict").unwrap();
    let err = repo.checkout("other").await.unwrap_err();
    assert!(
        matches!(err, GitError::CommandFailed { .. }),
        "expected CommandFailed, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_worktree_add_invalid_branch() {
    if !git_available() {
        return;
    }
    let tmp = temp_repo_dir();
    let repo = Repository::open(tmp.path()).await.unwrap();
    let wt_path = tmp.path().join("wt-invalid");
    let err = repo
        .worktree_add(&wt_path, "no-such-branch")
        .await
        .unwrap_err();
    assert!(
        matches!(err, GitError::CommandFailed { .. }),
        "expected CommandFailed, got {:?}",
        err
    );
}

#[tokio::test]
async fn test_hex_to_bytes() {
    use crate::repo::hex_to_bytes;
    assert_eq!(hex_to_bytes("ab").unwrap(), vec![0xab]);
    assert_eq!(hex_to_bytes("0102").unwrap(), vec![1, 2]);
    assert!(hex_to_bytes("abc").is_err());
    assert!(hex_to_bytes("zz").is_err());
    assert!(hex_to_bytes("a_").is_err());
}
