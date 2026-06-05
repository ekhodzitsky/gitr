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
    /// Initialize a new git repository
    Init {
        /// Directory to initialize
        path: PathBuf,
    },
    /// Clone a remote repository
    Clone {
        /// Remote URL
        url: String,
        /// Local destination path
        path: PathBuf,
    },
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
    /// Commit changes
    Commit {
        /// Commit message
        #[arg(short, long)]
        message: String,
        /// Commit all modified files (-a)
        #[arg(short, long)]
        all: bool,
        /// Skip pre-commit hooks (--no-verify)
        #[arg(long)]
        no_verify: bool,
        /// Specific paths to commit
        paths: Vec<PathBuf>,
    },
    /// Show unstaged diff
    Diff {
        /// Show shortstat instead of raw diff
        #[arg(long)]
        stat: bool,
        /// Show staged (cached) diff
        #[arg(long)]
        cached: bool,
        /// Output structured diff (hunks and lines)
        #[arg(long)]
        structured: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// List tracked, untracked or deleted files
    LsFiles {
        /// Include deleted files
        #[arg(long)]
        deleted: bool,
        /// Include untracked files
        #[arg(long)]
        others: bool,
        /// Respect .gitignore when listing untracked files
        #[arg(long)]
        exclude_standard: bool,
    },
    /// Grep the repository
    Grep {
        /// Search pattern
        pattern: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Worktree operations
    Worktree {
        #[command(subcommand)]
        cmd: WorktreeCommands,
    },
    /// Submodule operations
    Submodule {
        #[command(subcommand)]
        cmd: SubmoduleCommands,
    },
    /// Config operations
    Config {
        #[command(subcommand)]
        cmd: ConfigCommands,
    },
    /// Tag operations
    Tag {
        #[command(subcommand)]
        cmd: TagCommands,
    },
    /// Stash operations
    Stash {
        #[command(subcommand)]
        cmd: StashCommands,
    },
    /// Show file contents at a revision
    Show {
        /// File path
        path: String,
        /// Revision (defaults to HEAD)
        #[arg(short, long)]
        rev: Option<String>,
    },
    /// Show blame for a file
    Blame {
        /// File path
        path: String,
        /// Output structured blame (per-line authorship)
        #[arg(long)]
        structured: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Create an archive from a ref
    Archive {
        /// Git ref (branch, tag, commit)
        ref_name: String,
        /// Output archive path
        output: PathBuf,
    },
    /// Generate patch files for a commit range
    FormatPatch {
        /// Commit range (e.g. HEAD~3..HEAD)
        range: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Apply a patch file
    Apply {
        /// Path to the patch file
        patch: PathBuf,
        /// Only check if the patch can be applied (dry-run)
        #[arg(long)]
        check: bool,
    },
    /// Reflog operations
    Reflog {
        #[command(subcommand)]
        cmd: ReflogCommands,
    },
    /// Describe the current commit
    Describe {
        /// Use any tag, not just annotated tags
        #[arg(long)]
        tags: bool,
        /// Always use long format
        #[arg(long)]
        long: bool,
    },
    /// Remove untracked files
    Clean {
        /// Actually remove files (required by default)
        #[arg(short, long)]
        force: bool,
        /// Remove untracked directories too
        #[arg(short, long)]
        directories: bool,
        /// Dry-run: only show what would be removed
        #[arg(short, long)]
        dry_run: bool,
    },
    /// Git hooks management
    Hooks {
        #[command(subcommand)]
        cmd: HooksCommands,
    },
    /// Bisect operations
    Bisect {
        #[command(subcommand)]
        cmd: BisectCommands,
    },
    /// Git notes operations
    Notes {
        #[command(subcommand)]
        cmd: NotesCommands,
    },
    /// Check which paths are ignored
    CheckIgnore {
        /// Paths to check
        paths: Vec<PathBuf>,
    },
    /// Check git attributes
    CheckAttr {
        /// Paths to check
        paths: Vec<PathBuf>,
        /// Attributes to check
        #[arg(short, long)]
        attr: Vec<String>,
    },
    /// Bundle operations
    Bundle {
        #[command(subcommand)]
        cmd: BundleCommands,
    },
    /// Pull from a remote branch
    Pull {
        remote: String,
        branch: String,
        #[arg(long)]
        rebase: bool,
    },
    /// Switch to a branch
    Switch {
        branch: String,
        #[arg(long)]
        create: bool,
    },
    /// Restore working tree files
    Restore {
        paths: Vec<PathBuf>,
        #[arg(long)]
        staged: bool,
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Revert commits
    Revert {
        commits: Vec<String>,
        #[arg(long)]
        no_edit: bool,
    },
    /// Move or rename a tracked file
    Mv { source: PathBuf, dest: PathBuf },
    /// Remove tracked files
    Rm {
        paths: Vec<PathBuf>,
        #[arg(long)]
        cached: bool,
    },
    /// Find the best common ancestor between commits
    MergeBase { commits: Vec<String> },
    /// Resolve a revision to a full SHA
    RevParse { rev: String },
    /// List refs on a remote without fetching
    LsRemote { remote: String, refs: Vec<String> },
    /// Remote operations
    Remote {
        #[command(subcommand)]
        cmd: RemoteCommands,
    },
    /// Branch operations
    Branch {
        #[command(subcommand)]
        cmd: BranchCommands,
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
        /// Worktree path (defaults to `./<branch>`)
        #[arg(short, long)]
        path: Option<PathBuf>,
    },
    /// Prune stale worktrees
    Prune,
    /// Lock a worktree to prevent pruning
    Lock { path: PathBuf },
    /// Unlock a worktree
    Unlock { path: PathBuf },
    /// Move a worktree to a new location
    Move { old: PathBuf, new: PathBuf },
}

#[derive(Subcommand)]
enum SubmoduleCommands {
    /// List submodules
    List,
    /// Add a submodule
    Add {
        /// Submodule URL
        url: String,
        /// Local path
        path: PathBuf,
    },
    /// Update submodules
    Update {
        /// Initialize submodules
        #[arg(long)]
        init: bool,
        /// Update recursively
        #[arg(long)]
        recursive: bool,
    },
    /// Deinitialize a submodule
    Deinit {
        /// Submodule path
        path: PathBuf,
        /// Force deinit
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Get a config value
    Get {
        /// Config key
        key: String,
    },
    /// Set a config value
    Set {
        /// Config key
        key: String,
        /// Config value
        value: String,
    },
}

#[derive(Subcommand)]
enum TagCommands {
    /// List tags
    List,
    /// Create a tag
    Create {
        /// Tag name
        name: String,
        /// Tag message (creates annotated tag)
        #[arg(short, long)]
        message: Option<String>,
        /// Force overwrite existing tag
        #[arg(short, long)]
        force: bool,
    },
    /// Delete a tag
    Delete { name: String },
}

#[derive(Subcommand)]
enum StashCommands {
    /// List stash entries
    List,
    /// Drop a stash entry
    Drop { index: Option<usize> },
    /// Apply a stash entry without removing it
    Apply { index: Option<usize> },
    /// Show the diff of a stash entry
    Show { index: Option<usize> },
}

#[derive(Subcommand)]
enum ReflogCommands {
    /// List reflog entries
    List {
        /// Ref name (defaults to HEAD)
        #[arg(default_value = "HEAD")]
        ref_name: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Expire reflog entries
    Expire {
        /// Ref name
        ref_name: String,
        /// Expiration time (e.g. "2.weeks.ago")
        #[arg(short, long)]
        time: Option<String>,
    },
}

#[derive(Subcommand)]
enum HooksCommands {
    /// List installed hooks
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Install a hook
    Install {
        /// Hook name (e.g. pre-commit)
        name: String,
        /// Path to the script file
        script: PathBuf,
    },
    /// Remove a hook
    Remove {
        /// Hook name
        name: String,
    },
    /// Run a hook
    Run {
        /// Hook name
        name: String,
    },
}

#[derive(Subcommand)]
enum BisectCommands {
    /// Start bisect
    Start {
        /// Bad commit (defaults to HEAD)
        #[arg(short, long)]
        bad: Option<String>,
        /// Good commits
        #[arg(short, long)]
        good: Vec<String>,
    },
    /// Mark current commit as bad
    Bad {
        /// Commit (defaults to current)
        commit: Option<String>,
    },
    /// Mark current commit as good
    Good {
        /// Commit (defaults to current)
        commit: Option<String>,
    },
    /// Reset bisect
    Reset,
    /// Run a command automatically
    Run {
        /// Command to run
        command: String,
    },
}

#[derive(Subcommand)]
enum NotesCommands {
    /// List notes
    List {
        /// Note namespace
        #[arg(short, long)]
        namespace: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show note for an object
    Show {
        /// Object to show notes for
        object: String,
        /// Note namespace
        #[arg(short, long)]
        namespace: Option<String>,
    },
    /// Add a note
    Add {
        /// Object to attach note to
        object: String,
        /// Note message
        #[arg(short, long)]
        message: String,
        /// Note namespace
        #[arg(short, long)]
        namespace: Option<String>,
        /// Overwrite existing note
        #[arg(short, long)]
        force: bool,
    },
    /// Remove a note
    Remove {
        /// Object to remove note from
        object: String,
        /// Note namespace
        #[arg(short, long)]
        namespace: Option<String>,
    },
}

#[derive(Subcommand)]
enum BundleCommands {
    /// Create a bundle
    Create {
        /// Output bundle path
        output: PathBuf,
        /// Refs to include (defaults to --all)
        refs: Vec<String>,
    },
    /// List heads in a bundle
    ListHeads {
        /// Bundle path
        path: PathBuf,
    },
    /// Verify a bundle
    Verify {
        /// Bundle path
        path: PathBuf,
    },
    /// Unbundle into the repository
    Unbundle {
        /// Bundle path
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum RemoteCommands {
    /// Add a new remote
    Add { name: String, url: String },
    /// Remove a remote
    Remove { name: String },
    /// Rename a remote
    Rename { old: String, new: String },
}

#[derive(Subcommand)]
enum BranchCommands {
    /// Rename a branch
    Rename {
        old: String,
        new: String,
        #[arg(short, long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path } => {
            let repo = Repository::init(&path).await?;
            println!("initialized {}", repo.root().display());
        }
        Commands::Clone { url, path } => {
            let repo = Repository::clone(&url, &path).await?;
            println!("cloned into {}", repo.root().display());
        }
        Commands::Status { json } => {
            let repo = Repository::open(&cli.path).await?;
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
            let repo = Repository::open(&cli.path).await?;
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
            let repo = Repository::open(&cli.path).await?;
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
        Commands::Commit {
            message,
            all,
            no_verify,
            paths,
        } => {
            let repo = Repository::open(&cli.path).await?;
            let paths_ref: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
            let sha = if all || paths_ref.is_empty() {
                repo.commit(&message, &[] as &[&std::path::Path], no_verify)
                    .await?
            } else {
                repo.commit(&message, &paths_ref, no_verify).await?
            };
            println!("committed {sha}");
        }
        Commands::Diff {
            stat,
            cached,
            structured,
            json,
        } => {
            let repo = Repository::open(&cli.path).await?;
            if structured {
                let diffs = if cached {
                    repo.diff_cached_structured().await?
                } else {
                    repo.diff_structured().await?
                };
                if json {
                    println!("{}", serde_json::to_string_pretty(&diffs)?);
                } else {
                    for fd in diffs {
                        println!("--- {:?}", fd.old_path);
                        println!("+++ {:?}", fd.new_path);
                        for hunk in &fd.hunks {
                            println!(
                                "@@ -{},{} +{},{} @@ {}",
                                hunk.old_start,
                                hunk.old_lines,
                                hunk.new_start,
                                hunk.new_lines,
                                hunk.section
                            );
                            for line in &hunk.lines {
                                let prefix = match line.kind {
                                    gitr::DiffLineKind::Context => " ",
                                    gitr::DiffLineKind::Deletion => "-",
                                    gitr::DiffLineKind::Insertion => "+",
                                    gitr::DiffLineKind::NoNewline => "\\",
                                };
                                println!("{}{}", prefix, line.content);
                            }
                        }
                    }
                }
            } else if cached {
                let d = repo.diff_cached().await?;
                println!("{d}");
            } else if stat {
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
        Commands::LsFiles {
            deleted,
            others,
            exclude_standard,
        } => {
            let repo = Repository::open(&cli.path).await?;
            let files = repo.ls_files(deleted, others, exclude_standard).await?;
            for f in files {
                println!("{f}");
            }
        }
        Commands::Grep { pattern, json } => {
            let repo = Repository::open(&cli.path).await?;
            let hits = repo.grep(&pattern).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&hits)?);
            } else {
                for h in hits {
                    println!("{}:{}:{}", h.path, h.line, h.text);
                }
            }
        }
        Commands::Worktree { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
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
                WorktreeCommands::Prune => {
                    repo.worktree_prune().await?;
                    println!("worktree pruned");
                }
                WorktreeCommands::Lock { path } => {
                    repo.worktree_lock(&path).await?;
                    println!("worktree locked {}", path.display());
                }
                WorktreeCommands::Unlock { path } => {
                    repo.worktree_unlock(&path).await?;
                    println!("worktree unlocked {}", path.display());
                }
                WorktreeCommands::Move { old, new } => {
                    repo.worktree_move(&old, &new).await?;
                    println!("worktree moved {} -> {}", old.display(), new.display());
                }
            }
        }
        Commands::Submodule { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                SubmoduleCommands::List => {
                    let subs = repo.submodule_list().await?;
                    for s in subs {
                        let state = if s.uninitialized {
                            "uninit"
                        } else if s.dirty {
                            "dirty"
                        } else {
                            "ok"
                        };
                        println!("{} {} [{}]", &s.sha[..8.min(s.sha.len())], s.path, state);
                    }
                }
                SubmoduleCommands::Add { url, path } => {
                    repo.submodule_add(&url, &path).await?;
                    println!("submodule added {}", path.display());
                }
                SubmoduleCommands::Update { init, recursive } => {
                    repo.submodule_update(init, recursive).await?;
                    println!("submodules updated");
                }
                SubmoduleCommands::Deinit { path, force } => {
                    repo.submodule_deinit(&path, force).await?;
                    println!("submodule deinit {}", path.display());
                }
            }
        }
        Commands::Config { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                ConfigCommands::Get { key } => {
                    let val = repo.config_get(&key).await?;
                    match val {
                        Some(v) => println!("{v}"),
                        None => {
                            eprintln!("config key not found: {key}");
                            std::process::exit(1);
                        }
                    }
                }
                ConfigCommands::Set { key, value } => {
                    repo.config_set(&key, &value).await?;
                    println!("config set {key}={value}");
                }
            }
        }
        Commands::Tag { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                TagCommands::List => {
                    let tags = repo.tag_list().await?;
                    for t in tags {
                        println!("{} {}", t.name, &t.sha[..8.min(t.sha.len())]);
                    }
                }
                TagCommands::Create {
                    name,
                    message,
                    force,
                } => {
                    repo.tag_create(&name, message.as_deref(), force).await?;
                    println!("tag created {name}");
                }
                TagCommands::Delete { name } => {
                    repo.tag_delete(&name).await?;
                    println!("tag deleted {name}");
                }
            }
        }
        Commands::Stash { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                StashCommands::List => {
                    let list = repo.stash_list().await?;
                    for s in list {
                        println!(
                            "{} {} {}",
                            s.ref_name,
                            &s.sha[..8.min(s.sha.len())],
                            s.message
                        );
                    }
                }
                StashCommands::Drop { index } => {
                    repo.stash_drop(index).await?;
                    println!("stash dropped");
                }
                StashCommands::Apply { index } => {
                    repo.stash_apply(index).await?;
                    println!("stash applied");
                }
                StashCommands::Show { index } => {
                    let out = repo.stash_show(index).await?;
                    println!("{out}");
                }
            }
        }
        Commands::Show { path, rev } => {
            let repo = Repository::open(&cli.path).await?;
            let content = repo.show(&path, rev.as_deref()).await?;
            println!("{content}");
        }
        Commands::Blame {
            path,
            structured,
            json,
        } => {
            let repo = Repository::open(&cli.path).await?;
            if structured {
                let lines = repo.blame_structured(&path).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&lines)?);
                } else {
                    for line in lines {
                        println!(
                            "{} {} <{}> {}",
                            line.line_no, line.author, line.author_mail, line.content
                        );
                    }
                }
            } else {
                let raw = repo.blame(&path).await?;
                println!("{raw}");
            }
        }
        Commands::Archive { ref_name, output } => {
            let repo = Repository::open(&cli.path).await?;
            repo.archive(&ref_name, &output).await?;
            println!("archive created {}", output.display());
        }
        Commands::FormatPatch { range, json } => {
            let repo = Repository::open(&cli.path).await?;
            let patches = repo.format_patch(&range).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&patches)?);
            } else {
                for p in patches {
                    if let Some(subject) = p.subject {
                        println!("{subject}");
                    }
                    for fd in &p.diff {
                        for hunk in &fd.hunks {
                            for line in &hunk.lines {
                                let prefix = match line.kind {
                                    gitr::DiffLineKind::Context => " ",
                                    gitr::DiffLineKind::Deletion => "-",
                                    gitr::DiffLineKind::Insertion => "+",
                                    gitr::DiffLineKind::NoNewline => "\\",
                                };
                                println!("{}{}", prefix, line.content);
                            }
                        }
                    }
                }
            }
        }
        Commands::Apply { patch, check } => {
            let repo = Repository::open(&cli.path).await?;
            repo.apply_patch_file(&patch, check).await?;
            if check {
                println!("patch can be applied cleanly");
            } else {
                println!("patch applied");
            }
        }
        Commands::Describe { tags, long } => {
            let repo = Repository::open(&cli.path).await?;
            let desc = repo.describe(tags, long).await?;
            println!("{desc}");
        }
        Commands::Reflog { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                ReflogCommands::List { ref_name, json } => {
                    let entries = repo.reflog_list(Some(&ref_name)).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&entries)?);
                    } else {
                        for e in entries {
                            println!(
                                "{} {} <{}> {}",
                                e.designator, e.author, e.author_mail, e.subject
                            );
                        }
                    }
                }
                ReflogCommands::Expire { ref_name, time } => {
                    repo.reflog_expire(&ref_name, time.as_deref()).await?;
                    println!("reflog expired for {}", ref_name);
                }
            }
        }
        Commands::Clean {
            force,
            directories,
            dry_run,
        } => {
            let repo = Repository::open(&cli.path).await?;
            let removed = repo.clean(force, directories, dry_run).await?;
            for r in removed {
                println!("{r}");
            }
        }
        Commands::Hooks { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                HooksCommands::List { json } => {
                    let hooks = repo.hooks_list().await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&hooks)?);
                    } else {
                        for h in hooks {
                            let status = if h.active { "active" } else { "inactive" };
                            println!("{} ({})", h.name, status);
                        }
                    }
                }
                HooksCommands::Install { name, script } => {
                    let content = tokio::fs::read_to_string(&script).await?;
                    repo.hook_install(&name, &content).await?;
                    println!("hook {name} installed");
                }
                HooksCommands::Remove { name } => {
                    repo.hook_remove(&name).await?;
                    println!("hook {name} removed");
                }
                HooksCommands::Run { name } => {
                    let out = repo.run_hook(&name).await?;
                    println!("{}", out.stdout);
                    if !out.stderr.is_empty() {
                        eprintln!("{}", out.stderr);
                    }
                    if out.exit_code != 0 {
                        std::process::exit(out.exit_code);
                    }
                }
            }
        }
        Commands::Bisect { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                BisectCommands::Start { bad, good } => {
                    let good_refs: Vec<&str> = good.iter().map(|s| s.as_str()).collect();
                    let state = repo.bisect_start(bad.as_deref(), &good_refs).await?;
                    if let Some(current) = state.current {
                        println!("bisect started at {current}");
                    } else {
                        println!("bisect started");
                    }
                }
                BisectCommands::Bad { commit } => {
                    let state = repo.bisect_bad(commit.as_deref()).await?;
                    if let Some(current) = state.current {
                        println!("marked bad, now testing {current}");
                    }
                }
                BisectCommands::Good { commit } => {
                    let state = repo.bisect_good(commit.as_deref()).await?;
                    if let Some(current) = state.current {
                        println!("marked good, now testing {current}");
                    }
                }
                BisectCommands::Reset => {
                    repo.bisect_reset().await?;
                    println!("bisect reset");
                }
                BisectCommands::Run { command } => {
                    let out = repo.bisect_run(&command).await?;
                    println!("{out}");
                }
            }
        }
        Commands::Notes { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                NotesCommands::List { namespace, json } => {
                    let notes = repo.notes_list(namespace.as_deref()).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&notes)?);
                    } else {
                        for n in notes {
                            println!("{} -> {}", n.object, &n.commit[..8.min(n.commit.len())]);
                        }
                    }
                }
                NotesCommands::Show { object, namespace } => {
                    let content = repo.notes_show(&object, namespace.as_deref()).await?;
                    println!("{content}");
                }
                NotesCommands::Add {
                    object,
                    message,
                    namespace,
                    force,
                } => {
                    repo.notes_add(&message, &object, namespace.as_deref(), force)
                        .await?;
                    println!("note added to {object}");
                }
                NotesCommands::Remove { object, namespace } => {
                    repo.notes_remove(&object, namespace.as_deref()).await?;
                    println!("note removed from {object}");
                }
            }
        }
        Commands::CheckIgnore { paths } => {
            let repo = Repository::open(&cli.path).await?;
            let ignored = repo.check_ignore(&paths).await?;
            for p in ignored {
                println!("{p}");
            }
        }
        Commands::CheckAttr { paths, attr } => {
            let repo = Repository::open(&cli.path).await?;
            let attrs: Vec<&str> = attr.iter().map(|s| s.as_str()).collect();
            let results = repo.check_attr(&paths, &attrs).await?;
            for r in results {
                println!("{}: {} = {}", r.path, r.attr, r.value);
            }
        }
        Commands::Bundle { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                BundleCommands::Create { output, refs } => {
                    let refs_ref: Vec<&str> = refs.iter().map(|s| s.as_str()).collect();
                    let refs_opt = if refs_ref.is_empty() {
                        None
                    } else {
                        Some(refs_ref.as_slice())
                    };
                    repo.bundle_create(&output, refs_opt).await?;
                    println!("bundle created {}", output.display());
                }
                BundleCommands::ListHeads { path } => {
                    let heads = repo.bundle_list_heads(&path).await?;
                    for h in heads {
                        println!("{h}");
                    }
                }
                BundleCommands::Verify { path } => {
                    repo.bundle_verify(&path).await?;
                    println!("bundle verified {}", path.display());
                }
                BundleCommands::Unbundle { path } => {
                    let refs = repo.bundle_unbundle(&path).await?;
                    for r in refs {
                        println!("{r}");
                    }
                }
            }
        }
        Commands::Pull {
            remote,
            branch,
            rebase,
        } => {
            let repo = Repository::open(&cli.path).await?;
            repo.pull(&remote, &branch, rebase).await?;
            println!("pulled {remote}/{branch}");
        }
        Commands::Switch { branch, create } => {
            let repo = Repository::open(&cli.path).await?;
            repo.switch(&branch, create).await?;
            println!("switched to {branch}");
        }
        Commands::Restore {
            paths,
            staged,
            source,
        } => {
            let repo = Repository::open(&cli.path).await?;
            let paths_ref: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
            repo.restore(&paths_ref, staged, source.as_deref()).await?;
            println!("restored {} file(s)", paths.len());
        }
        Commands::Revert { commits, no_edit } => {
            let repo = Repository::open(&cli.path).await?;
            let commits_ref: Vec<&str> = commits.iter().map(|s| s.as_str()).collect();
            repo.revert(&commits_ref, no_edit).await?;
            println!("reverted {} commit(s)", commits.len());
        }
        Commands::Mv { source, dest } => {
            let repo = Repository::open(&cli.path).await?;
            repo.mv(&source, &dest).await?;
            println!("moved {} -> {}", source.display(), dest.display());
        }
        Commands::Rm { paths, cached } => {
            let repo = Repository::open(&cli.path).await?;
            let paths_ref: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
            repo.rm(&paths_ref, cached).await?;
            println!("removed {} file(s)", paths.len());
        }
        Commands::MergeBase { commits } => {
            let repo = Repository::open(&cli.path).await?;
            let commits_ref: Vec<&str> = commits.iter().map(|s| s.as_str()).collect();
            let base = repo.merge_base(&commits_ref).await?;
            println!("{base}");
        }
        Commands::RevParse { rev } => {
            let repo = Repository::open(&cli.path).await?;
            let sha = repo.rev_parse(&rev).await?;
            println!("{sha}");
        }
        Commands::LsRemote { remote, refs } => {
            let repo = Repository::open(&cli.path).await?;
            let refs_ref: Vec<&str> = refs.iter().map(|s| s.as_str()).collect();
            let refs_opt = if refs_ref.is_empty() {
                None
            } else {
                Some(refs_ref.as_slice())
            };
            let list = repo.ls_remote(&remote, refs_opt).await?;
            for (sha, ref_name) in list {
                println!("{sha} {ref_name}");
            }
        }
        Commands::Remote { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                RemoteCommands::Add { name, url } => {
                    repo.remote_add(&name, &url).await?;
                    println!("remote added {name}");
                }
                RemoteCommands::Remove { name } => {
                    repo.remote_remove(&name).await?;
                    println!("remote removed {name}");
                }
                RemoteCommands::Rename { old, new } => {
                    repo.remote_rename(&old, &new).await?;
                    println!("remote renamed {old} -> {new}");
                }
            }
        }
        Commands::Branch { cmd } => {
            let repo = Repository::open(&cli.path).await?;
            match cmd {
                BranchCommands::Rename { old, new, force } => {
                    repo.branch_rename(&old, &new, force).await?;
                    println!("branch renamed {old} -> {new}");
                }
            }
        }
    }

    Ok(())
}
