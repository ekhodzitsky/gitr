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
            let sha = repo
                .commit(message, &[] as &[&std::path::Path])
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({ "sha": sha }))
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
        _ => Err(format!("unknown method: {method}")),
    }
}
