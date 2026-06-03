use anyhow::Result;
use clap::{Parser, Subcommand};
use gitr::Repository;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gitr")]
#[command(about = "Async typed git CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Repository path (defaults to current directory)
    #[arg(short, long, global = true, default_value = ".")]
    path: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Show working tree status
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// CI-friendly repository health check
    Check,
    /// Show commit log
    Log {
        /// Limit number of commits
        #[arg(short = 'n', long)]
        max_count: Option<usize>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Worktree operations
    Worktree {
        #[command(subcommand)]
        cmd: WorktreeCommands,
    },
    /// Show unstaged diff
    Diff {
        /// Show shortstat instead of raw diff
        #[arg(long)]
        stat: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum WorktreeCommands {
    /// List worktrees
    List {
        #[arg(long)]
        json: bool,
    },
    /// Create or switch to a worktree
    Switch {
        branch: String,
        /// Worktree path (defaults to ./<branch>)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo = Repository::open(&cli.path).await?;

    match cli.command {
        Commands::Status { json } => {
            let status = repo.status().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                for f in &status.staged {
                    println!("staged: {f}");
                }
                for f in &status.unstaged {
                    println!("unstaged: {f}");
                }
                for f in &status.untracked {
                    println!("untracked: {f}");
                }
            }
        }
        Commands::Check => {
            let clean = repo.ensure_clean().await.is_ok();
            let conflicts = repo.is_merge_conflict().await?;
            let untracked = repo.has_untracked_files().await?;
            let ok = clean && !conflicts && !untracked;
            let result = serde_json::json!({
                "clean": clean,
                "conflicts": conflicts,
                "untracked": untracked,
                "ok": ok,
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
            if !ok {
                std::process::exit(1);
            }
        }
        Commands::Log { max_count, json } => {
            let log = repo.log(max_count).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&log)?);
            } else {
                for entry in log {
                    println!(
                        "{} {} {}",
                        &entry.short_sha, &entry.timestamp, &entry.message
                    );
                }
            }
        }
        Commands::Worktree { cmd } => match cmd {
            WorktreeCommands::List { json } => {
                let list = repo.worktree_list().await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    for wt in list {
                        println!("{} -> {}", wt.path.display(), wt.branch);
                    }
                }
            }
            WorktreeCommands::Switch { branch, path } => {
                let path = path.unwrap_or_else(|| PathBuf::from(&branch));
                if !repo.branch_exists(&branch).await? {
                    repo.branch_create(&branch, None).await?;
                }
                repo.worktree_add(&path, &branch).await?;
                println!("worktree {} -> {}", path.display(), branch);
            }
        },
        Commands::Diff { stat, json } => {
            if stat {
                let s = repo.diff_shortstat().await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&s)?);
                } else {
                    println!(
                        "{} files changed, {} insertions(+), {} deletions(-)",
                        s.files_changed, s.insertions, s.deletions
                    );
                }
            } else {
                let d = repo.diff().await?;
                println!("{d}");
            }
        }
    }

    Ok(())
}
