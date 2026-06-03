use criterion::{criterion_group, criterion_main, Criterion};
use gitr::Repository;
use std::process::Command;
use tempfile::TempDir;

fn setup_repo() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "bench@test.com"])
        .current_dir(dir)
        .status()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Bench"])
        .current_dir(dir)
        .status()
        .unwrap();
    std::fs::write(dir.join("file.txt"), "hello").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .status()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .status()
        .unwrap();
    tmp
}

fn bench_status(c: &mut Criterion) {
    let tmp = setup_repo();
    let dir = tmp.path().to_str().unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = rt.block_on(async { Repository::open(dir).await.unwrap() });

    let mut group = c.benchmark_group("status");
    group.bench_function("gitr", |b| {
        b.to_async(&rt)
            .iter(|| async { repo.status().await.unwrap() })
    });
    group.bench_function("subprocess", |b| {
        b.iter(|| {
            Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(dir)
                .output()
                .unwrap()
        })
    });
    group.finish();
}

fn bench_current_branch(c: &mut Criterion) {
    let tmp = setup_repo();
    let dir = tmp.path().to_str().unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = rt.block_on(async { Repository::open(dir).await.unwrap() });

    let mut group = c.benchmark_group("current_branch");
    group.bench_function("gitr", |b| {
        b.to_async(&rt)
            .iter(|| async { repo.current_branch().await.unwrap() })
    });
    group.bench_function("subprocess", |b| {
        b.iter(|| {
            Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(dir)
                .output()
                .unwrap()
        })
    });
    group.finish();
}

fn bench_log(c: &mut Criterion) {
    let tmp = setup_repo();
    let dir = tmp.path().to_str().unwrap();

    // create 50 commits
    for i in 0..50 {
        std::fs::write(tmp.path().join("file.txt"), format!("hello {i}")).unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", &format!("commit {i}")])
            .current_dir(dir)
            .status()
            .unwrap();
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = rt.block_on(async { Repository::open(dir).await.unwrap() });

    let mut group = c.benchmark_group("log_50_commits");
    group.bench_function("gitr", |b| {
        b.to_async(&rt)
            .iter(|| async { repo.log(Some(50)).await.unwrap() })
    });
    group.bench_function("subprocess", |b| {
        b.iter(|| {
            Command::new("git")
                .args(["log", "-n", "50", "--format=%H|%s|%an|%at"])
                .current_dir(dir)
                .output()
                .unwrap()
        })
    });
    group.finish();
}

criterion_group!(benches, bench_status, bench_current_branch, bench_log);
criterion_main!(benches);
