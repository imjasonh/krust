//! Integration tests for authentication

use anyhow::Result;
use krust::auth::{resolve_auth, AuthConfig};
use oci_distribution::secrets::RegistryAuth;
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

    // Test GitHub Container Registry auth
    let ghcr_auth = resolve_auth("ghcr.io/user/image:tag")?;
    match ghcr_auth {
        RegistryAuth::Basic(user, pass) => {
            // The base64 "dGVzdDp0ZXN0MTIz" decodes to "test:test123"
            assert_eq!(user, "test");
            assert_eq!(pass, "test123");
        }
        _ => panic!("Expected Basic auth for ghcr.io"),
    }

    // Test Docker Hub auth
    let docker_auth = resolve_auth("docker.io/library/ubuntu:latest")?;
    match docker_auth {
        RegistryAuth::Basic(user, pass) => {
            assert_eq!(user, "testuser");
            assert_eq!(pass, "testpass");
        }
        _ => panic!("Expected Basic auth for docker.io"),
    }

    // Test unknown registry returns anonymous
    let unknown_auth = resolve_auth("unknown.registry.io/image:tag")?;
    assert!(matches!(unknown_auth, RegistryAuth::Anonymous));

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
