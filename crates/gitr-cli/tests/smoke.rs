use std::process::Command;

#[test]
#[cfg(not(windows))]
fn cli_help_shows_usage() {
    let bin = env!("CARGO_BIN_EXE_gitr");
    let output = Command::new(bin)
        .arg("--help")
        .output()
        .expect("failed to run gitr --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Async typed git CLI"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("check"));
}
