use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn mcp_git_status_responds() {
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
            r#"{{"jsonrpc":"2.0","id":1,"method":"git_status","params":{{}}}}"#
        )
        .unwrap();
    }

    let output = child.wait_with_output().expect("failed to read stdout");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next().expect("no output from mcp");
    let resp: serde_json::Value = serde_json::from_str(line).expect("invalid json");
    assert!(resp.get("result").is_some(), "expected result in response");
}
