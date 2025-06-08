//! Integration tests for authentication

use anyhow::Result;
use krust::auth::{AuthConfig, DefaultKeychain, Keychain};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_auth_integration_with_docker_config() -> Result<()> {
    // Create a temporary directory for Docker config
    let temp_dir = TempDir::new()?;
    let config_dir = temp_dir.path().join(".docker");
    fs::create_dir_all(&config_dir)?;

    // Create a test Docker config
    let config_content = r#"{
        "auths": {
            "ghcr.io": {
                "auth": "dGVzdDp0ZXN0MTIz"
            },
            "docker.io": {
                "username": "testuser",
                "password": "testpass"
            }
        }
    }"#;

    fs::write(config_dir.join("config.json"), config_content)?;

    // Set HOME to temp directory
    std::env::set_var("HOME", temp_dir.path());

    let keychain = DefaultKeychain::new();

    // Test GitHub Container Registry auth
    let ghcr_auth = keychain.resolve("ghcr.io/user/image:tag")?;
    let ghcr_config = ghcr_auth.authorization()?;
    assert!(!ghcr_config.is_anonymous());
    assert_eq!(ghcr_config.auth, Some("dGVzdDp0ZXN0MTIz".to_string()));

    // Test Docker Hub auth
    let docker_auth = keychain.resolve("docker.io/library/ubuntu:latest")?;
    let docker_config = docker_auth.authorization()?;
    assert!(!docker_config.is_anonymous());
    assert_eq!(docker_config.username, Some("testuser".to_string()));
    assert_eq!(docker_config.password, Some("testpass".to_string()));

    // Test unknown registry returns anonymous
    let unknown_auth = keychain.resolve("unknown.registry.io/image:tag")?;
    let unknown_config = unknown_auth.authorization()?;
    assert!(unknown_config.is_anonymous());

    // Clean up
    std::env::remove_var("HOME");

    Ok(())
}

#[test]
fn test_auth_config_creation() {
    // Test anonymous
    let anon = AuthConfig::anonymous();
    assert!(anon.is_anonymous());

    // Test basic auth
    let basic = AuthConfig::new("user".to_string(), "pass".to_string());
    assert!(!basic.is_anonymous());
    assert_eq!(basic.username, Some("user".to_string()));
    assert_eq!(basic.password, Some("pass".to_string()));

    // Test auth header generation
    let header = basic.to_authorization_header().unwrap();
    assert!(header.is_some());
    let header_val = header.unwrap();
    assert!(header_val.starts_with("Basic "));
}
