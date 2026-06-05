use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
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

fn setup_repo_with_untracked(n: usize) -> TempDir {
    let tmp = setup_repo();
    let dir = tmp.path();
    for i in 0..n {
        std::fs::write(dir.join(format!("untracked_{i}.txt")), "content").unwrap();
    }
    tmp
}

fn setup_repo_with_commits(n: usize) -> TempDir {
    let tmp = setup_repo();
    let dir = tmp.path();
    for i in 0..n {
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
    tmp
}

fn setup_repo_with_diff() -> TempDir {
    let tmp = setup_repo();
    let dir = tmp.path();
    std::fs::write(dir.join("file.txt"), "world").unwrap();
    tmp
}

fn bench_status_untracked(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("status_untracked");

    for size in [10usize, 100, 1000] {
        let tmp = setup_repo_with_untracked(size);
        let dir = tmp.path().to_str().unwrap();
        let repo = rt.block_on(async { Repository::open(dir).await.unwrap() });

        group.bench_with_input(BenchmarkId::new("gitr", size), &repo, |b, repo| {
            b.to_async(&rt)
                .iter(|| async { repo.status().await.unwrap() })
        });
    }

    group.finish();
}

fn bench_log(c: &mut Criterion) {
    let tmp = setup_repo_with_commits(50);
    let dir = tmp.path().to_str().unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = rt.block_on(async { Repository::open(dir).await.unwrap() });

    let mut group = c.benchmark_group("log");
    group.bench_function("gitr_none", |b| {
        b.to_async(&rt)
            .iter(|| async { repo.log(None).await.unwrap() })
    });
    group.finish();
}

fn bench_diff(c: &mut Criterion) {
    let tmp = setup_repo_with_diff();
    let dir = tmp.path().to_str().unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let repo = rt.block_on(async { Repository::open(dir).await.unwrap() });

    let mut group = c.benchmark_group("diff");
    group.bench_function("gitr", |b| {
        b.to_async(&rt)
            .iter(|| async { repo.diff().await.unwrap() })
    });
    group.finish();
}

fn bench_parse_status(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_status");

    for size in [10usize, 100, 1000] {
        let mut output = String::new();
        for i in 0..size {
            output.push_str(&format!("?? untracked_{i}.txt\n"));
        }

        group.bench_with_input(BenchmarkId::new("sample", size), &output, |b, output| {
            b.iter(|| gitr::parse::parse_status(output).unwrap())
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_status_untracked,
    bench_log,
    bench_diff,
    bench_parse_status
);
criterion_main!(benches);
