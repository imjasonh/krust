//! Integration tests for credential helper functionality

use anyhow::Result;
use krust::auth::resolve_auth;
use krust::registry::RegistryAuth;
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
    let old_xdg_runtime = env::var("XDG_RUNTIME_DIR").ok();
    let old_home = env::var("HOME").ok();

    // Set to empty directory and clear all possible config locations
    env::set_var("DOCKER_CONFIG", tmp_dir.path());
    env::remove_var("REGISTRY_AUTH_FILE");
    env::remove_var("XDG_RUNTIME_DIR");
    // Set HOME to temp dir to avoid ~/.docker/config.json
    env::set_var("HOME", tmp_dir.path());

    // Should resolve to anonymous
    let auth = resolve_auth("docker.io/library/alpine")?;

    // Debug output for CI
    if !matches!(auth, RegistryAuth::Anonymous) {
        eprintln!("Expected Anonymous auth but got: {:?}", auth);
        eprintln!("DOCKER_CONFIG: {:?}", env::var("DOCKER_CONFIG"));
        eprintln!("REGISTRY_AUTH_FILE: {:?}", env::var("REGISTRY_AUTH_FILE"));
        eprintln!("HOME: {:?}", env::var("HOME"));

        // Check if there's a Docker config in the temp dir
        let config_path = tmp_dir.path().join("config.json");
        eprintln!(
            "Config exists at {:?}: {}",
            config_path,
            config_path.exists()
        );
    }

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
    if let Some(val) = old_xdg_runtime {
        env::set_var("XDG_RUNTIME_DIR", val);
    } else {
        env::remove_var("XDG_RUNTIME_DIR");
    }
    if let Some(val) = old_home {
        env::set_var("HOME", val);
    } else {
        env::remove_var("HOME");
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
    let old_xdg_runtime = env::var("XDG_RUNTIME_DIR").ok();
    let old_home = env::var("HOME").ok();

    // Set our test config and clear other env vars
    env::set_var("DOCKER_CONFIG", tmp_dir.path());
    env::remove_var("REGISTRY_AUTH_FILE");
    env::remove_var("XDG_RUNTIME_DIR");
    env::set_var("HOME", tmp_dir.path());

    // TODO: Implement actual credential resolution from Docker config
    // For now, should resolve to anonymous auth
    let auth = resolve_auth("test.registry.io/myimage")?;
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
    if let Some(val) = old_xdg_runtime {
        env::set_var("XDG_RUNTIME_DIR", val);
    } else {
        env::remove_var("XDG_RUNTIME_DIR");
    }
    if let Some(val) = old_home {
        env::set_var("HOME", val);
    } else {
        env::remove_var("HOME");
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
    let old_xdg_runtime = env::var("XDG_RUNTIME_DIR").ok();
    let old_home = env::var("HOME").ok();

    // Set our test config and clear other env vars
    env::set_var("DOCKER_CONFIG", tmp_dir.path());
    env::remove_var("REGISTRY_AUTH_FILE");
    env::remove_var("XDG_RUNTIME_DIR");
    env::set_var("HOME", tmp_dir.path());

    // TODO: Implement actual credential resolution from Docker config
    // For now, should resolve to anonymous auth
    let auth = resolve_auth("ghcr.io/user/image")?;
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
    if let Some(val) = old_xdg_runtime {
        env::set_var("XDG_RUNTIME_DIR", val);
    } else {
        env::remove_var("XDG_RUNTIME_DIR");
    }
    if let Some(val) = old_home {
        env::set_var("HOME", val);
    } else {
        env::remove_var("HOME");
    }

    Ok(())
}
