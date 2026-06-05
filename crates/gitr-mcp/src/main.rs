use gitr::Repository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[tokio::main]
async fn main() {
    let repo_path = std::env::var("GITR_REPO_PATH").unwrap_or_else(|_| ".".to_string());
    let repo = match Repository::open(&repo_path).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to open repo: {e}");
            std::process::exit(1);
        }
    };

    let stdin = io::BufReader::new(io::stdin());
    let mut stdout = io::stdout();
    let mut lines = stdin.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                write_response(
                    &mut stdout,
                    JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("parse error: {e}"),
                            data: None,
                        }),
                    },
                )
                .await;
                continue;
            }
        };

        let result = handle_request(&repo, &req.method, req.params.clone()).await;
        let resp = match result {
            Ok(val) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: Some(val),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32000,
                    message: e,
                    data: None,
                }),
            },
        };

        write_response(&mut stdout, resp).await;
    }
}

async fn write_response(stdout: &mut io::Stdout, resp: JsonRpcResponse) {
    let json = match serde_json::to_string(&resp) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("failed to serialize response: {e}");
            return;
        }
    };
    if let Err(e) = stdout.write_all(json.as_bytes()).await {
        eprintln!("failed to write response: {e}");
        return;
    }
    if let Err(e) = stdout.write_all(b"\n").await {
        eprintln!("failed to write newline: {e}");
        return;
    }
    if let Err(e) = stdout.flush().await {
        eprintln!("failed to flush stdout: {e}");
    }
}

async fn handle_request(repo: &Repository, method: &str, params: Value) -> Result<Value, String> {
    match method {
        "git_status" => {
            let status = repo.status().await.map_err(|e| e.to_string())?;
            serde_json::to_value(status).map_err(|e| e.to_string())
        }
        "git_check" => {
            let clean = repo.ensure_clean().await.is_ok();
            let conflicts = repo.is_merge_conflict().await.map_err(|e| e.to_string())?;
            let untracked = repo
                .has_untracked_files()
                .await
                .map_err(|e| e.to_string())?;
            let val = serde_json::json!({
                "clean": clean,
                "conflicts": conflicts,
                "untracked": untracked,
            });
            Ok(val)
        }
        "git_log" => {
            let max_count = params
                .get("max_count")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let log = repo.log(max_count).await.map_err(|e| e.to_string())?;
            serde_json::to_value(log).map_err(|e| e.to_string())
        }
        "git_branch_current" => {
            let branch = repo.current_branch().await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "branch": branch }))
        }
        "git_checkout" => {
            let branch = params
                .get("branch")
                .and_then(|v| v.as_str())
                .ok_or("missing branch")?;
            repo.checkout(branch).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_commit" => {
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or("missing message")?;
            let no_verify = params
                .get("no_verify")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let sha = repo
                .commit(message, &[] as &[&std::path::Path], no_verify)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "sha": sha }))
        }
        "git_commit_signed" => {
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or("missing message")?;
            let gpg_key = params.get("gpg_key").and_then(|v| v.as_str());
            let no_verify = params
                .get("no_verify")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.commit_signed(&[], message, gpg_key, no_verify)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_verify_commit" => {
            let sha = params
                .get("sha")
                .and_then(|v| v.as_str())
                .ok_or("missing sha")?;
            let v = repo.verify_commit(sha).await.map_err(|e| e.to_string())?;
            serde_json::to_value(v).map_err(|e| e.to_string())
        }
        "git_worktree_list" => {
            let list = repo.worktree_list().await.map_err(|e| e.to_string())?;
            serde_json::to_value(list).map_err(|e| e.to_string())
        }
        "git_worktree_add" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            let branch = params
                .get("branch")
                .and_then(|v| v.as_str())
                .ok_or("missing branch")?;
            let wt = repo
                .worktree_add(&path, branch)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(wt).map_err(|e| e.to_string())
        }
        "git_submodule_list" => {
            let list = repo.submodule_list().await.map_err(|e| e.to_string())?;
            serde_json::to_value(list).map_err(|e| e.to_string())
        }
        "git_submodule_add" => {
            let url = params
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or("missing url")?;
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            repo.submodule_add(url, &path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_submodule_update" => {
            let init = params
                .get("init")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let recursive = params
                .get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.submodule_update(init, recursive)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_config_get" => {
            let key = params
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or("missing key")?;
            let val = repo.config_get(key).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "value": val }))
        }
        "git_config_set" => {
            let key = params
                .get("key")
                .and_then(|v| v.as_str())
                .ok_or("missing key")?;
            let value = params
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or("missing value")?;
            repo.config_set(key, value)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_tag_list" => {
            let list = repo.tag_list().await.map_err(|e| e.to_string())?;
            serde_json::to_value(list).map_err(|e| e.to_string())
        }
        "git_tag_create" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("missing name")?;
            let message = params.get("message").and_then(|v| v.as_str());
            let force = params
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.tag_create(name, message, force)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_stash_list" => {
            let list = repo.stash_list().await.map_err(|e| e.to_string())?;
            serde_json::to_value(list).map_err(|e| e.to_string())
        }
        "git_show" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing path")?;
            let rev = params.get("rev").and_then(|v| v.as_str());
            let content = repo.show(path, rev).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "content": content }))
        }
        "git_blame_structured" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing path")?;
            let lines = repo
                .blame_structured(path)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(lines).map_err(|e| e.to_string())
        }
        "git_grep" => {
            let pattern = params
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or("missing pattern")?;
            let hits = repo.grep(pattern).await.map_err(|e| e.to_string())?;
            serde_json::to_value(hits).map_err(|e| e.to_string())
        }
        "git_ls_files" => {
            let deleted = params
                .get("deleted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let others = params
                .get("others")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let exclude_standard = params
                .get("exclude_standard")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let files = repo
                .ls_files(deleted, others, exclude_standard)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(files).map_err(|e| e.to_string())
        }
        "git_diff_cached" => {
            let diff = repo.diff_cached().await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "diff": diff }))
        }
        "git_diff_structured" => {
            let cached = params
                .get("cached")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let diffs = if cached {
                repo.diff_cached_structured().await
            } else {
                repo.diff_structured().await
            }
            .map_err(|e| e.to_string())?;
            serde_json::to_value(diffs).map_err(|e| e.to_string())
        }
        "git_archive" => {
            let ref_name = params
                .get("ref")
                .and_then(|v| v.as_str())
                .ok_or("missing ref")?;
            let output = params
                .get("output")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing output")?;
            repo.archive(ref_name, &output)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_reset" => {
            let mode_str = params
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("mixed");
            let mode = match mode_str {
                "soft" => gitr::ResetMode::Soft,
                "hard" => gitr::ResetMode::Hard,
                _ => gitr::ResetMode::Mixed,
            };
            let target = params.get("target").and_then(|v| v.as_str());
            repo.reset(mode, target).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_cherry_pick" => {
            let commits: Vec<&str> = params
                .get("commits")
                .and_then(|v| v.as_array())
                .ok_or("missing commits")?
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            repo.cherry_pick(&commits)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_describe" => {
            let tags = params
                .get("tags")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let long = params
                .get("long")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let desc = repo.describe(tags, long).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "describe": desc }))
        }
        "git_clean" => {
            let force = params
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let directories = params
                .get("directories")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let dry_run = params
                .get("dry_run")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let removed = repo
                .clean(force, directories, dry_run)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(removed).map_err(|e| e.to_string())
        }
        "git_clone" => {
            let url = params
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or("missing url")?;
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            let r = Repository::clone(url, &path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "root": r.root().to_string_lossy(),
            }))
        }
        "git_format_patch" => {
            let range = params
                .get("range")
                .and_then(|v| v.as_str())
                .ok_or("missing range")?;
            let patches = repo.format_patch(range).await.map_err(|e| e.to_string())?;
            serde_json::to_value(patches).map_err(|e| e.to_string())
        }
        "git_apply_patch" => {
            let patch = params
                .get("patch")
                .and_then(|v| v.as_str())
                .ok_or("missing patch")?;
            let dry_run = params
                .get("dry_run")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.apply_patch(patch, dry_run)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_reflog_list" => {
            let ref_name = params.get("ref").and_then(|v| v.as_str());
            let entries = repo
                .reflog_list(ref_name)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(entries).map_err(|e| e.to_string())
        }
        "git_reflog_expire" => {
            let ref_name = params
                .get("ref")
                .and_then(|v| v.as_str())
                .ok_or("missing ref")?;
            let time = params.get("time").and_then(|v| v.as_str());
            repo.reflog_expire(ref_name, time)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_hooks_list" => {
            let hooks = repo.hooks_list().await.map_err(|e| e.to_string())?;
            serde_json::to_value(hooks).map_err(|e| e.to_string())
        }
        "git_hook_install" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("missing name")?;
            let script = params
                .get("script")
                .and_then(|v| v.as_str())
                .ok_or("missing script")?;
            repo.hook_install(name, script)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_hook_remove" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("missing name")?;
            repo.hook_remove(name).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_run_hook" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("missing name")?;
            let out = repo.run_hook(name).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "stdout": out.stdout,
                "stderr": out.stderr,
                "exit_code": out.exit_code,
            }))
        }
        "git_bisect_start" => {
            let bad = params.get("bad").and_then(|v| v.as_str());
            let good: Vec<&str> = params
                .get("good")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let state = repo
                .bisect_start(bad, &good)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(state).map_err(|e| e.to_string())
        }
        "git_bisect_bad" => {
            let commit = params.get("commit").and_then(|v| v.as_str());
            let state = repo.bisect_bad(commit).await.map_err(|e| e.to_string())?;
            serde_json::to_value(state).map_err(|e| e.to_string())
        }
        "git_bisect_good" => {
            let commit = params.get("commit").and_then(|v| v.as_str());
            let state = repo.bisect_good(commit).await.map_err(|e| e.to_string())?;
            serde_json::to_value(state).map_err(|e| e.to_string())
        }
        "git_bisect_reset" => {
            repo.bisect_reset().await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_bisect_run" => {
            let command = params
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or("missing command")?;
            let out = repo.bisect_run(command).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "output": out }))
        }
        "git_notes_list" => {
            let namespace = params.get("namespace").and_then(|v| v.as_str());
            let notes = repo
                .notes_list(namespace)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(notes).map_err(|e| e.to_string())
        }
        "git_notes_show" => {
            let object = params
                .get("object")
                .and_then(|v| v.as_str())
                .ok_or("missing object")?;
            let namespace = params.get("namespace").and_then(|v| v.as_str());
            let content = repo
                .notes_show(object, namespace)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "content": content }))
        }
        "git_notes_add" => {
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or("missing message")?;
            let object = params
                .get("object")
                .and_then(|v| v.as_str())
                .ok_or("missing object")?;
            let namespace = params.get("namespace").and_then(|v| v.as_str());
            let force = params
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.notes_add(message, object, namespace, force)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_notes_remove" => {
            let object = params
                .get("object")
                .and_then(|v| v.as_str())
                .ok_or("missing object")?;
            let namespace = params.get("namespace").and_then(|v| v.as_str());
            repo.notes_remove(object, namespace)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_check_ignore" => {
            let paths: Vec<PathBuf> = params
                .get("paths")
                .and_then(|v| v.as_array())
                .ok_or("missing paths")?
                .iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect();
            let ignored = repo.check_ignore(&paths).await.map_err(|e| e.to_string())?;
            serde_json::to_value(ignored).map_err(|e| e.to_string())
        }
        "git_check_attr" => {
            let paths: Vec<PathBuf> = params
                .get("paths")
                .and_then(|v| v.as_array())
                .ok_or("missing paths")?
                .iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect();
            let attrs: Vec<&str> = params
                .get("attrs")
                .and_then(|v| v.as_array())
                .ok_or("missing attrs")?
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            let results = repo
                .check_attr(&paths, &attrs)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(results).map_err(|e| e.to_string())
        }
        "git_bundle_create" => {
            let output = params
                .get("output")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing output")?;
            let refs: Vec<&str> = params
                .get("refs")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let refs_opt = if refs.is_empty() {
                None
            } else {
                Some(refs.as_slice())
            };
            repo.bundle_create(&output, refs_opt)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_bundle_list_heads" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            let heads = repo
                .bundle_list_heads(&path)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(heads).map_err(|e| e.to_string())
        }
        "git_bundle_verify" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            repo.bundle_verify(&path).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_bundle_unbundle" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            let refs = repo
                .bundle_unbundle(&path)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(refs).map_err(|e| e.to_string())
        }
        "git_init" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            let r = Repository::init(&path).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "root": r.root().to_string_lossy(),
            }))
        }
        "git_pull" => {
            let remote = params
                .get("remote")
                .and_then(|v| v.as_str())
                .ok_or("missing remote")?;
            let branch = params
                .get("branch")
                .and_then(|v| v.as_str())
                .ok_or("missing branch")?;
            let rebase = params
                .get("rebase")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.pull(remote, branch, rebase)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_switch" => {
            let branch = params
                .get("branch")
                .and_then(|v| v.as_str())
                .ok_or("missing branch")?;
            let create = params
                .get("create")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.switch(branch, create)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_restore" => {
            let paths: Vec<PathBuf> = params
                .get("paths")
                .and_then(|v| v.as_array())
                .ok_or("missing paths")?
                .iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect();
            let staged = params
                .get("staged")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let source = params.get("source").and_then(|v| v.as_str());
            let path_refs: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
            repo.restore(&path_refs, staged, source)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_revert" => {
            let commits: Vec<&str> = params
                .get("commits")
                .and_then(|v| v.as_array())
                .ok_or("missing commits")?
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            let no_edit = params
                .get("no_edit")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.revert(&commits, no_edit)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_stash_drop" => {
            let index = params
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            repo.stash_drop(index).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_stash_apply" => {
            let index = params
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            repo.stash_apply(index).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_stash_show" => {
            let index = params
                .get("index")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);
            let diff = repo.stash_show(index).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "diff": diff }))
        }
        "git_remote_add" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("missing name")?;
            let url = params
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or("missing url")?;
            repo.remote_add(name, url)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_remote_remove" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("missing name")?;
            repo.remote_remove(name).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_remote_rename" => {
            let old = params
                .get("old")
                .and_then(|v| v.as_str())
                .ok_or("missing old")?;
            let new = params
                .get("new")
                .and_then(|v| v.as_str())
                .ok_or("missing new")?;
            repo.remote_rename(old, new)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_branch_rename" => {
            let old = params
                .get("old")
                .and_then(|v| v.as_str())
                .ok_or("missing old")?;
            let new = params
                .get("new")
                .and_then(|v| v.as_str())
                .ok_or("missing new")?;
            let force = params
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            repo.branch_rename(old, new, force)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_tag_delete" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("missing name")?;
            repo.tag_delete(name).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_worktree_prune" => {
            repo.worktree_prune().await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_worktree_lock" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            repo.worktree_lock(&path).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_worktree_unlock" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing path")?;
            repo.worktree_unlock(&path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_worktree_move" => {
            let old_path = params
                .get("old_path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing old_path")?;
            let new_path = params
                .get("new_path")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing new_path")?;
            repo.worktree_move(&old_path, &new_path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_mv" => {
            let source = params
                .get("source")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing source")?;
            let dest = params
                .get("dest")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .ok_or("missing dest")?;
            repo.mv(&source, &dest).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_rm" => {
            let paths: Vec<PathBuf> = params
                .get("paths")
                .and_then(|v| v.as_array())
                .ok_or("missing paths")?
                .iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect();
            let cached = params
                .get("cached")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let path_refs: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();
            repo.rm(&path_refs, cached)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!(null))
        }
        "git_merge_base" => {
            let commits: Vec<&str> = params
                .get("commits")
                .and_then(|v| v.as_array())
                .ok_or("missing commits")?
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            let sha = repo.merge_base(&commits).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "sha": sha }))
        }
        "git_rev_parse" => {
            let rev = params
                .get("rev")
                .and_then(|v| v.as_str())
                .ok_or("missing rev")?;
            let sha = repo.rev_parse(rev).await.map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "sha": sha }))
        }
        "git_ls_remote" => {
            let remote = params
                .get("remote")
                .and_then(|v| v.as_str())
                .ok_or("missing remote")?;
            let refs: Vec<&str> = params
                .get("refs")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let refs_opt = if refs.is_empty() {
                None
            } else {
                Some(refs.as_slice())
            };
            let result = repo
                .ls_remote(remote, refs_opt)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(result).map_err(|e| e.to_string())
        }
        _ => Err(format!("unknown method: {method}")),
    }
}
