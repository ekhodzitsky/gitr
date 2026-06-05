use std::io::Write;
use std::process::{Command, Stdio};

fn mcp_request(method: &str, params: &str) -> serde_json::Value {
    let bin = env!("CARGO_BIN_EXE_gitr-mcp");
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();

    let mut child = Command::new(bin)
        .env("GITR_REPO_PATH", &repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn gitr-mcp");

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"{method}","params":{params}}}"#
        )
        .unwrap();
    }

    let output = child.wait_with_output().expect("failed to read stdout");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next().expect("no output from mcp");
    serde_json::from_str(line).expect("invalid json")
}

#[test]
fn mcp_git_status_responds() {
    let resp = mcp_request("git_status", "{}");
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "expected result or error in response"
    );
}

#[test]
fn mcp_git_log_responds() {
    let resp = mcp_request("git_log", "{}");
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "expected result or error in response"
    );
}

#[test]
fn mcp_git_grep_responds() {
    let resp = mcp_request("git_grep", r#"{"pattern":"fn"}"#);
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "expected result or error in response"
    );
}

#[test]
fn mcp_git_config_get_responds() {
    let resp = mcp_request("git_config_get", r#"{"key":"user.name"}"#);
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "expected result or error in response"
    );
}

#[test]
fn mcp_git_tag_list_responds() {
    let resp = mcp_request("git_tag_list", "{}");
    assert!(
        resp.get("result").is_some() || resp.get("error").is_some(),
        "expected result or error in response"
    );
}
