use gitr::Repository;
use std::path::PathBuf;

fn git_binary() -> PathBuf {
    std::env::var_os("GIT_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "git".into())
}

fn configure_git(dir: &std::path::Path) {
    std::process::Command::new(git_binary())
        .current_dir(dir)
        .args(["config", "user.email", "test@gitr.rs"])
        .status()
        .expect("git config email");
    std::process::Command::new(git_binary())
        .current_dir(dir)
        .args(["config", "user.name", "gitr test"])
        .status()
        .expect("git config name");
}

#[tokio::test]
#[ignore = "requires git binary"]
async fn init_and_status() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    std::process::Command::new(git_binary())
        .current_dir(dir)
        .arg("init")
        .status()
        .expect("git init");

    configure_git(dir);

    let repo = Repository::open(dir).await.unwrap();
    let status = repo.status().await.unwrap();
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
}

#[tokio::test]
#[ignore = "requires git binary"]
async fn worktree_add_and_remove() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    std::process::Command::new(git_binary())
        .current_dir(dir)
        .arg("init")
        .status()
        .expect("git init");

    configure_git(dir);

    // Create initial commit so we can create branches.
    std::fs::write(dir.join("README.md"), "# test\n").unwrap();
    std::process::Command::new(git_binary())
        .current_dir(dir)
        .args(["add", "README.md"])
        .status()
        .unwrap();
    std::process::Command::new(git_binary())
        .current_dir(dir)
        .args(["commit", "-m", "initial"])
        .status()
        .unwrap();

    let repo = Repository::open(dir).await.unwrap();
    let wt_path = dir.join("wt-1");
    repo.worktree_add(&wt_path, "feature-x").await.unwrap();

    let wts = repo.worktree_list().await.unwrap();
    assert!(wts.iter().any(|wt| wt.path == wt_path));

    repo.worktree_remove(&wt_path, false).await.unwrap();
}
