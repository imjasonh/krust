use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use std::env;
use std::process::Command as StdCommand;

// Helper to get the appropriate test platform based on runtime architecture
fn get_test_platform() -> &'static str {
    // In CI, test the native platform
    if env::var("CI").is_ok() {
        if cfg!(target_arch = "x86_64") {
            "linux/amd64"
        } else if cfg!(target_arch = "aarch64") {
            "linux/arm64"
        } else {
            "linux/amd64"
        }
    } else {
        // For local development, always use amd64 as it's most commonly available
        "linux/amd64"
    }
}

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
        .arg(".") // Explicitly pass current directory
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
        .arg("--platform")
        .arg(get_test_platform())
        .arg(".") // Explicitly pass current directory
        .env("KRUST_REPO", "test.local")
        .current_dir(&example_dir);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Building Rust project"))
        .stderr(predicate::str::contains(
            "Successfully built image for 1 platform(s)",
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
        .arg("--platform")
        .arg(get_test_platform())
        .arg("--image")
        .arg("test.local/hello:latest")
        .arg(".") // Explicitly pass current directory
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
        .arg("--platform")
        .arg(get_test_platform())
        .arg("--image")
        .arg("test.local/hello:latest")
        .arg(".") // Explicitly pass current directory
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
        .arg(".") // Explicitly pass current directory
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
fn test_single_platform_build() -> Result<()> {
    let example_dir = env::current_dir()?.join("example").join("hello-krust");
    let platform = get_test_platform();

    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("build")
        .arg("--no-push")
        .arg("--platform")
        .arg(platform)
        .arg("--image")
        .arg("test.local/single-platform:latest")
        .arg(".")
        .current_dir(&example_dir);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains(format!(
            "Building for platform: {}",
            platform
        )))
        .stderr(predicate::str::contains(
            "Successfully built image for 1 platform(s)",
        ));
    Ok(())
}

#[test]
fn test_multi_platform_build() -> Result<()> {
    let example_dir = env::current_dir()?.join("example").join("hello-krust");
    let mut cmd = Command::cargo_bin("krust")?;

    // Test building for multiple platforms
    // This will use whatever targets are available
    cmd.arg("build")
        .arg("--no-push")
        .arg("--platform")
        .arg("linux/amd64,linux/arm64")
        .arg("--image")
        .arg("test.local/multi-platform:latest")
        .arg(".")
        .current_dir(&example_dir);

    // The command might fail if targets aren't installed, but we should test that it tries
    let output = cmd.output()?;

    if output.status.success() {
        // If it succeeds, verify we built for multiple platforms
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Building for platform: linux/amd64"));
        assert!(stderr.contains("Building for platform: linux/arm64"));
        assert!(stderr.contains("Successfully built image for 2 platform(s)"));
    } else {
        // If it fails, it should be because of missing targets
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("target may not be installed")
                || stderr.contains("linker")
                || stderr.contains("cross-compilation")
                || stderr.contains("not found"),
            "Build failed for unexpected reason: {}",
            stderr
        );
    }
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

#[test]
fn test_platform_detection_from_base_image() -> Result<()> {
    let example_dir = env::current_dir()?.join("example").join("hello-krust");

    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("build")
        .arg("--no-push")
        .arg("--image")
        .arg("test.local/platform-detection:latest")
        .arg(".")
        .current_dir(&example_dir)
        .env("RUST_LOG", "info");

    // The default base image (cgr.dev/chainguard/static:latest) supports multiple platforms
    // so we should see platform detection happening
    cmd.assert()
        .success()
        .stderr(predicate::str::contains(
            "Detecting available platforms from base image",
        ))
        .stderr(predicate::str::contains("Detected platforms"));
    Ok(())
}

#[test]
fn test_explicit_platform_overrides_detection() -> Result<()> {
    let example_dir = env::current_dir()?.join("example").join("hello-krust");
    let platform = get_test_platform();

    let mut cmd = Command::cargo_bin("krust")?;
    cmd.arg("build")
        .arg("--no-push")
        .arg("--platform")
        .arg(platform)
        .arg("--image")
        .arg("test.local/explicit-platform:latest")
        .arg(".")
        .current_dir(&example_dir)
        .env("RUST_LOG", "info");

    // When platform is explicitly specified, we should NOT see platform detection
    let output = cmd.output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success());
    assert!(!stderr.contains("Detecting available platforms from base image"));
    assert!(stderr.contains(&format!("Building for platform: {}", platform)));
    Ok(())
}

#[test]
fn test_alpine_base_image_many_platforms() -> Result<()> {
    // Create a temporary directory for this test
    let temp_dir = tempfile::tempdir()?;
    let test_project_dir = temp_dir.path().join("test-alpine");
    std::fs::create_dir_all(&test_project_dir)?;

    // Create a simple Cargo.toml with Alpine as base image
    let cargo_toml = r#"[package]
name = "test-alpine"
version = "0.1.0"
edition = "2021"

[dependencies]

[package.metadata.krust]
base-image = "alpine:latest"
"#;
    std::fs::write(test_project_dir.join("Cargo.toml"), cargo_toml)?;

    // Create src directory and main.rs
    std::fs::create_dir_all(test_project_dir.join("src"))?;
    std::fs::write(
        test_project_dir.join("src/main.rs"),
        r#"fn main() { println!("Hello from Alpine test!"); }"#,
    )?;

    let mut cmd = Command::cargo_bin("krust")?;
    let output = cmd
        .arg("build")
        .arg("--no-push")
        .arg("--image")
        .arg("test.local/alpine-platforms:latest")
        .arg(".")
        .current_dir(&test_project_dir)
        .env("RUST_LOG", "info")
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Alpine typically supports many platforms, but we might not have all the toolchains
    // So we check that platform detection happened
    assert!(stderr.contains("Detecting available platforms from base image: alpine:latest"));
    assert!(stderr.contains("Found platforms:") || stderr.contains("Detected platforms:"));

    // The build might fail due to missing toolchains for some platforms, which is OK
    // We're mainly testing that platform detection works
    if !output.status.success() {
        // Check if it failed due to missing toolchains (expected)
        assert!(
            stderr.contains("target may not be installed")
                || stderr.contains("linker")
                || stderr.contains("cross-compilation"),
            "Build failed for unexpected reason: {}",
            stderr
        );
    }

    Ok(())
}
