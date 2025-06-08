use std::process::Command;

#[test]
fn test_help_flag() {
    let output = Command::new("cargo")
        .args(&["run", "--", "--version"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("krust"));
    assert!(output.status.success());
}

#[test]
fn test_basic_run() {
    let output = Command::new("cargo")
        .arg("run")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hello from krust!"));
    assert!(output.status.success());
}