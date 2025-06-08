use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use std::env;
use std::process::Command as StdCommand;

#[test]
fn test_version_command() -> Result<()> {
    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("krust 0.1.0"));
    Ok(())
}

#[test]
fn test_version_subcommand() -> Result<()> {
    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("krust 0.1.0"));
    Ok(())
}

#[test]
fn test_help_command() -> Result<()> {
    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("--help");
    cmd.assert().success().stdout(predicate::str::contains(
        "A container image build tool for Rust applications",
    ));
    Ok(())
}

#[test]
fn test_build_help() -> Result<()> {
    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("build").arg("--help");
    cmd.assert().success().stdout(predicate::str::contains(
        "Build a container image from a Rust application",
    ));
    Ok(())
}

#[test]
fn test_build_requires_repo_or_image() -> Result<()> {
    // Use the example directory
    let example_dir = env::current_dir()?.join("example").join("hello-krust");

    // Run krust build without KRUST_REPO or --image
    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("build")
        .arg("--no-push")
        .current_dir(&example_dir)
        .env_remove("KRUST_REPO");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Either --image or KRUST_REPO must be set",
    ));
    Ok(())
}

#[test]
fn test_build_with_krust_repo_env() -> Result<()> {
    // Use the example directory
    let example_dir = env::current_dir()?.join("example").join("hello-krust");

    // Run krust build with KRUST_REPO env var
    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("build")
        .arg("--no-push")
        .env("KRUST_REPO", "test.local")
        .current_dir(&example_dir);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Building Rust project"))
        .stderr(predicate::str::contains(
            "Successfully built image: test.local/hello-krust:latest",
        ));
    Ok(())
}

#[test]
fn test_command_substitution_syntax() -> Result<()> {
    // Get the example project directory
    let example_dir = env::current_dir()?.join("example").join("hello-krust");

    // Test that output is clean for command substitution
    let mut cmd = Command::cargo_bin("krust")?;
    let output = cmd
        .arg("build")
        .arg("--no-push")
        .arg("--image")
        .arg("test.local/hello:latest")
        .current_dir(&example_dir)
        .output()?;

    // Stdout should be empty when --no-push is used
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "Stdout should be empty with --no-push"
    );

    // Stderr should contain log messages
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Building Rust project"));
    assert!(stderr.contains("Successfully built image"));
    Ok(())
}

#[test]
fn test_verbose_logging() -> Result<()> {
    let example_dir = env::current_dir()?.join("example").join("hello-krust");

    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("--verbose")
        .arg("build")
        .arg("--no-push")
        .arg("--image")
        .arg("test.local/hello:latest")
        .current_dir(&example_dir);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("DEBUG"));
    Ok(())
}

#[test]
fn test_full_build_and_run_workflow() -> Result<()> {
    // This test requires Docker
    let docker_check = StdCommand::new("docker").arg("version").output();
    match docker_check {
        Ok(output) if output.status.success() => {
            // Docker is available, proceed with test
        }
        _ => {
            // Docker not available or not working
            panic!("Docker is required for this test but is not available");
        }
    }

    // Get the example project directory
    let example_dir = env::current_dir()?.join("example").join("hello-krust");

    // Build and push to ttl.sh
    let mut cmd = Command::cargo_bin("krust")?;
    let output = cmd
        .arg("build")
        .env("KRUST_REPO", "ttl.sh/krust-test")
        .current_dir(&example_dir)
        .output()?;

    assert!(output.status.success(), "Build failed");

    // Get the image reference from stdout
    let image_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(image_ref.starts_with("ttl.sh/krust-test/hello-krust@sha256:"));

    // Try to run the image
    let docker_output = StdCommand::new("docker")
        .args(&["run", "--rm", &image_ref])
        .output()?;

    assert!(docker_output.status.success(), "Docker run failed");
    let docker_stdout = String::from_utf8_lossy(&docker_output.stdout);
    assert!(docker_stdout.contains("Hello from krust example!"));
    Ok(())
}
