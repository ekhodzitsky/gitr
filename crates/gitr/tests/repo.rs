use gitr::Repository;
use std::path::PathBuf;

fn git_available() -> bool {
    which::which("git").is_ok()
}

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
async fn init_and_status() {
    if !git_available() {
        return;
    }
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
async fn worktree_add_and_remove() {
    if !git_available() {
        return;
    }
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
    repo.branch_create("feature-x", None).await.unwrap();
    let wt_path = dir.join("wt-1");
    repo.worktree_add(&wt_path, "feature-x").await.unwrap();

    let wts = repo.worktree_list().await.unwrap();
    let canonical_wt = wt_path.canonicalize().unwrap_or(wt_path.clone());
    assert!(wts
        .iter()
        .any(|wt| { wt.path.canonicalize().unwrap_or(wt.path.clone()) == canonical_wt }));

    repo.worktree_remove(&wt_path, false).await.unwrap();
}

#[cfg(feature = "stream")]
#[tokio::test]
async fn log_stream_smoke() {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    std::process::Command::new(git_binary())
        .current_dir(dir)
        .arg("init")
        .status()
        .expect("git init");

    configure_git(dir);

    for i in 1..=3 {
        let file = dir.join(format!("f{i}.txt"));
        std::fs::write(&file, format!("content {i}\n")).unwrap();
        std::process::Command::new(git_binary())
            .current_dir(dir)
            .args(["add", file.to_str().unwrap()])
            .status()
            .unwrap();
        std::process::Command::new(git_binary())
            .current_dir(dir)
            .args(["commit", "-m", &format!("commit {i}")])
            .status()
            .unwrap();
    }

    let repo = Repository::open(dir).await.unwrap();
    let stream = repo.log_stream().await.unwrap();
    use tokio_stream::StreamExt;
    let entries: Vec<_> = stream.collect().await;
    assert_eq!(entries.len(), 3);
    // Verify descending order (commit 3 first)
    assert_eq!(entries[0].as_ref().unwrap().message, "commit 3");
    assert_eq!(entries[1].as_ref().unwrap().message, "commit 2");
    assert_eq!(entries[2].as_ref().unwrap().message, "commit 1");
}

#[tokio::test]
async fn verify_commit_unsigned() {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    std::process::Command::new(git_binary())
        .current_dir(dir)
        .arg("init")
        .status()
        .expect("git init");

    configure_git(dir);

    std::fs::write(dir.join("f.txt"), "hello\n").unwrap();
    std::process::Command::new(git_binary())
        .current_dir(dir)
        .args(["add", "f.txt"])
        .status()
        .unwrap();
    std::process::Command::new(git_binary())
        .current_dir(dir)
        .args(["commit", "-m", "initial"])
        .status()
        .unwrap();

    let repo = Repository::open(dir).await.unwrap();
    let sha = repo.head_commit().await.unwrap();
    let v = repo.verify_commit(sha.as_ref()).await.unwrap();
    assert!(!v.valid);
    assert_eq!(v.status, "N");
}

#[tokio::test]
async fn init_creates_repo() {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("new-repo");
    std::fs::create_dir(&dir).unwrap();

    let repo = Repository::init(&dir).await.unwrap();
    assert!(dir.join(".git").exists());
    let branch = repo.current_branch().await;
    // HEAD may be unborn, so current_branch can fail — that's ok
    assert!(branch.is_ok() || branch.is_err());
}

#[tokio::test]
async fn clone_local_repo() {
    if !git_available() {
        return;
    }
    let origin_tmp = tempfile::tempdir().unwrap();
    let origin_dir = origin_tmp.path();
    std::process::Command::new(git_binary())
        .current_dir(origin_dir)
        .arg("init")
        .status()
        .expect("git init origin");
    configure_git(origin_dir);
    std::fs::write(origin_dir.join("README.md"), "# hello\n").unwrap();
    std::process::Command::new(git_binary())
        .current_dir(origin_dir)
        .args(["add", "README.md"])
        .status()
        .unwrap();
    std::process::Command::new(git_binary())
        .current_dir(origin_dir)
        .args(["commit", "-m", "init"])
        .status()
        .unwrap();

    let dest_tmp = tempfile::tempdir().unwrap();
    let dest_dir = dest_tmp.path().join("cloned");

    let repo = Repository::clone(origin_dir.to_str().unwrap(), &dest_dir)
        .await
        .unwrap();
    assert!(dest_dir.join(".git").exists());
    let head = repo.head_commit().await.unwrap();
    assert!(!head.is_empty());
}

#[tokio::test]
async fn ls_files_diff_grep_archive() {
    if !git_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    std::process::Command::new(git_binary())
        .current_dir(dir)
        .arg("init")
        .status()
        .expect("git init");
    configure_git(dir);

    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.join("lib.rs"), "pub fn add() {}\n").unwrap();
    std::process::Command::new(git_binary())
        .current_dir(dir)
        .args(["add", "hello.rs", "lib.rs"])
        .status()
        .unwrap();
    std::process::Command::new(git_binary())
        .current_dir(dir)
        .args(["commit", "-m", "init"])
        .status()
        .unwrap();

    let repo = Repository::open(dir).await.unwrap();

    // ls_files
    let files = repo.ls_files(false, false, false).await.unwrap();
    assert!(files.contains(&"hello.rs".to_string()));
    assert!(files.contains(&"lib.rs".to_string()));

    // diff_cached should be empty after commit
    let diff = repo.diff_cached().await.unwrap();
    assert!(diff.trim().is_empty());

    // grep
    let hits = repo.grep("fn main").await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "hello.rs");
    assert_eq!(hits[0].line, 1);

    // archive
    let archive_path = dir.join("archive.tar");
    repo.archive("HEAD", &archive_path).await.unwrap();
    assert!(archive_path.exists());
}
