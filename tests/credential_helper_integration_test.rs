//! Integration tests for credential helper functionality

use anyhow::Result;
use krust::auth::resolve_auth;
use oci_distribution::secrets::RegistryAuth;
use std::env;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_resolve_auth_anonymous() -> Result<()> {
    // Create empty temp directory for Docker config
    let tmp_dir = TempDir::new()?;

    // Save current env vars
    let old_docker_config = env::var("DOCKER_CONFIG").ok();
    let old_registry_auth = env::var("REGISTRY_AUTH_FILE").ok();

    // Set to empty directory
    env::set_var("DOCKER_CONFIG", tmp_dir.path());
    env::remove_var("REGISTRY_AUTH_FILE");

    // Should resolve to anonymous
    let auth = resolve_auth("docker.io/library/alpine")?;
    assert!(matches!(auth, RegistryAuth::Anonymous));

    // Restore env vars
    if let Some(val) = old_docker_config {
        env::set_var("DOCKER_CONFIG", val);
    } else {
        env::remove_var("DOCKER_CONFIG");
    }
    if let Some(val) = old_registry_auth {
        env::set_var("REGISTRY_AUTH_FILE", val);
    } else {
        env::remove_var("REGISTRY_AUTH_FILE");
    }

    Ok(())
}

#[test]
fn test_resolve_auth_from_config() -> Result<()> {
    let tmp_dir = TempDir::new()?;
    let config_path = tmp_dir.path().join("config.json");

    // Create a test config
    let config = r#"{
        "auths": {
            "test.registry.io": {
                "username": "testuser",
                "password": "testpass"
            }
        }
    }"#;

    fs::write(&config_path, config)?;

    // Save current env vars
    let old_docker_config = env::var("DOCKER_CONFIG").ok();
    let old_registry_auth = env::var("REGISTRY_AUTH_FILE").ok();

    // Set our test config and clear other env vars
    env::set_var("DOCKER_CONFIG", tmp_dir.path());
    env::remove_var("REGISTRY_AUTH_FILE");

    // Should resolve to basic auth
    let auth = resolve_auth("test.registry.io/myimage")?;
    assert!(matches!(auth, RegistryAuth::Basic(user, pass)
        if user == "testuser" && pass == "testpass"));

    // Restore env vars
    if let Some(val) = old_docker_config {
        env::set_var("DOCKER_CONFIG", val);
    } else {
        env::remove_var("DOCKER_CONFIG");
    }
    if let Some(val) = old_registry_auth {
        env::set_var("REGISTRY_AUTH_FILE", val);
    } else {
        env::remove_var("REGISTRY_AUTH_FILE");
    }

    Ok(())
}

#[test]
fn test_resolve_auth_bearer_token() -> Result<()> {
    let tmp_dir = TempDir::new()?;
    let config_path = tmp_dir.path().join("config.json");

    // Create a test config with bearer token
    let config = r#"{
        "auths": {
            "ghcr.io": {
                "registrytoken": "test-bearer-token"
            }
        }
    }"#;

    fs::write(&config_path, config)?;

    // Save current env vars
    let old_docker_config = env::var("DOCKER_CONFIG").ok();
    let old_registry_auth = env::var("REGISTRY_AUTH_FILE").ok();

    // Set our test config and clear other env vars
    env::set_var("DOCKER_CONFIG", tmp_dir.path());
    env::remove_var("REGISTRY_AUTH_FILE");

    // Should resolve to bearer auth
    let auth = resolve_auth("ghcr.io/user/image")?;
    assert!(matches!(auth, RegistryAuth::Bearer(token)
        if token == "test-bearer-token"));

    // Restore env vars
    if let Some(val) = old_docker_config {
        env::set_var("DOCKER_CONFIG", val);
    } else {
        env::remove_var("DOCKER_CONFIG");
    }
    if let Some(val) = old_registry_auth {
        env::set_var("REGISTRY_AUTH_FILE", val);
    } else {
        env::remove_var("REGISTRY_AUTH_FILE");
    }

    Ok(())
}
