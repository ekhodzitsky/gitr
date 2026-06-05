use std::process::Command;

#[test]
fn cli_help_shows_usage() {
    let bin = env!("CARGO_BIN_EXE_gitr");
    let output = Command::new(bin)
        .arg("--help")
        .output()
        .expect("failed to run gitr --help");
    if !output.status.success() {
        eprintln!("gitr --help exited with: {:?}", output.status.code());
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    }
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Async typed git CLI"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("check"));
}
