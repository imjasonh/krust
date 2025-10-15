//! Integration tests for krust
//!
//! Most integration tests have been converted to testscripts in tests/testdata/*.txt
//! These tests remain here because they require Docker to run and verify the built images.

use anyhow::Result;
use assert_cmd::Command;
use std::env;
use std::process::Command as StdCommand;

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
    // For the full workflow test, build for the actual native platform so Docker can run it
    let native_platform = if cfg!(target_arch = "aarch64") {
        "linux/arm64"
    } else {
        "linux/amd64"
    };

    let mut cmd = Command::cargo_bin("krust")?;
    let output = cmd
        .arg("build")
        .arg("--platform")
        .arg(native_platform)
        .arg(".")
        .env("KRUST_REPO", "ttl.sh/krust-test")
        .current_dir(&example_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Build failed: {}", stderr);
    }

    // Get the image reference from stdout
    let image_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(image_ref.starts_with("ttl.sh/krust-test/hello-krust@sha256:"));

    // Try to run the image
    let docker_output = StdCommand::new("docker")
        .args(&["run", "--rm", &image_ref])
        .output()?;

    if !docker_output.status.success() {
        let stderr = String::from_utf8_lossy(&docker_output.stderr);
        panic!("Docker run failed with image {}: {}", image_ref, stderr);
    }

    let docker_stdout = String::from_utf8_lossy(&docker_output.stdout);
    assert!(docker_stdout.contains("Hello from krust example!"));
    Ok(())
}

#[test]
fn test_multi_arch_build_and_run() -> Result<()> {
    // This test requires Docker
    let docker_check = StdCommand::new("docker").arg("version").output();
    match docker_check {
        Ok(output) if output.status.success() => {
            // Docker is available, proceed with test
        }
        _ => {
            eprintln!("Docker is required for this test but is not available");
            return Ok(());
        }
    }

    let example_dir = env::current_dir()?.join("example").join("hello-krust");

    // Build multi-arch image and push to ttl.sh
    let mut cmd = Command::cargo_bin("krust")?;
    let output = cmd
        .arg("build")
        .arg("--platform")
        .arg("linux/amd64,linux/arm64")
        .arg(".")
        .env("KRUST_REPO", "ttl.sh/krust-multiarch-test")
        .current_dir(&example_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If it fails due to missing targets, that's expected in some environments
        if stderr.contains("target may not be installed")
            || stderr.contains("linker")
            || stderr.contains("cross-compilation")
            || stderr.contains("not found")
        {
            eprintln!("Skipping multi-arch run test - build failed due to missing toolchain");
            return Ok(());
        }
        panic!("Build failed unexpectedly: {}", stderr);
    }

    // Get the image reference from stdout
    let image_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(image_ref.starts_with("ttl.sh/krust-multiarch-test/hello-krust"));

    // Try to run the image - it should work on the current architecture
    let docker_output = StdCommand::new("docker")
        .args(&["run", "--rm", &image_ref])
        .output()?;

    assert!(docker_output.status.success(), "Docker run failed");
    let docker_stdout = String::from_utf8_lossy(&docker_output.stdout);
    assert!(docker_stdout.contains("Hello from krust example!"));

    Ok(())
}
