use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;

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
