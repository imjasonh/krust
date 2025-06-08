//! Tests for the auth module

use super::*;
use crate::auth::keychain::MultiKeychain;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a test Docker config file
fn create_test_config(dir: &Path, content: &str) -> PathBuf {
    let config_path = dir.join("config.json");
    fs::write(&config_path, content).unwrap();
    config_path
}

#[test]
fn test_docker_config_parsing() {
    let config_json = r#"{
        "auths": {
            "docker.io": {
                "auth": "dXNlcjpwYXNz"
            },
            "gcr.io": {
                "username": "oauth2accesstoken",
                "password": "ya29.token",
                "registrytoken": "bearer-token"
            }
        },
        "credHelpers": {
            "ecr.amazonaws.com": "ecr-login"
        },
        "credsStore": "osxkeychain"
    }"#;

    let config: DockerConfig = serde_json::from_str(config_json).unwrap();

    assert_eq!(config.auths.len(), 2);
    assert!(config.auths.contains_key("docker.io"));
    assert!(config.auths.contains_key("gcr.io"));

    let docker_auth = &config.auths["docker.io"];
    assert_eq!(docker_auth.auth, Some("dXNlcjpwYXNz".to_string()));

    let gcr_auth = &config.auths["gcr.io"];
    assert_eq!(gcr_auth.username, Some("oauth2accesstoken".to_string()));
    assert_eq!(gcr_auth.password, Some("ya29.token".to_string()));
    assert_eq!(gcr_auth.registry_token, Some("bearer-token".to_string()));

    assert_eq!(config.cred_helpers.len(), 1);
    assert_eq!(config.cred_helpers["ecr.amazonaws.com"], "ecr-login");

    assert_eq!(config.creds_store, Some("osxkeychain".to_string()));
}

#[test]
fn test_docker_auth_entry_to_auth_config() {
    let entry = DockerAuthEntry {
        auth: Some("dXNlcjpwYXNz".to_string()),
        username: None,
        password: None,
        identity_token: None,
        registry_token: None,
    };

    let config = entry.to_auth_config();
    assert_eq!(config.auth, Some("dXNlcjpwYXNz".to_string()));
    assert!(config.username.is_none());
    assert!(config.password.is_none());
}

#[test]
fn test_default_keychain_config_paths() {
    let temp_dir = TempDir::new().unwrap();
    let config_content = r#"{
        "auths": {
            "test.registry.io": {
                "auth": "dGVzdDp0ZXN0"
            }
        }
    }"#;

    // Test DOCKER_CONFIG env var
    let docker_config_dir = temp_dir.path().join("docker");
    fs::create_dir_all(&docker_config_dir).unwrap();
    create_test_config(&docker_config_dir, config_content);

    std::env::set_var("DOCKER_CONFIG", docker_config_dir.to_str().unwrap());

    let keychain = DefaultKeychain::new();
    let auth = keychain.resolve("test.registry.io/image:tag").unwrap();
    let auth_config = auth.authorization().unwrap();

    assert!(!auth_config.is_anonymous());
    assert_eq!(auth_config.auth, Some("dGVzdDp0ZXN0".to_string()));

    std::env::remove_var("DOCKER_CONFIG");
}

// Registry extraction tests are skipped as the method is private
// The functionality is tested through the public resolve() method

#[test]
fn test_keychain_resolve_different_registries() {
    // Test that keychain can resolve different registry formats
    let keychain = DefaultKeychain::new();

    // Should return anonymous for unknown registries
    let auth = keychain.resolve("unknown.registry.io/image:tag").unwrap();
    let config = auth.authorization().unwrap();
    assert!(config.is_anonymous());
}

#[test]
fn test_anonymous_authenticator() {
    let anon = Anonymous;
    let auth = anon.authorization().unwrap();
    assert!(auth.is_anonymous());
    assert!(auth.to_authorization_header().unwrap().is_none());
}

#[test]
fn test_basic_authenticator() {
    let basic = Basic::new("user".to_string(), "pass".to_string());
    let auth = basic.authorization().unwrap();
    assert!(!auth.is_anonymous());

    let header = auth.to_authorization_header().unwrap().unwrap();
    assert!(header.starts_with("Basic "));
}

#[test]
fn test_bearer_authenticator() {
    let bearer = Bearer::new("token123".to_string());
    let auth = bearer.authorization().unwrap();
    assert!(!auth.is_anonymous());

    let header = auth.to_authorization_header().unwrap().unwrap();
    assert_eq!(header, "Bearer token123");
}

#[test]
fn test_multi_keychain() {
    // Create a keychain that always returns anonymous
    struct AlwaysAnonymous;
    impl Keychain for AlwaysAnonymous {
        fn resolve(&self, _: &str) -> Result<Box<dyn Authenticator>> {
            Ok(Box::new(Anonymous))
        }
    }

    // Create a keychain that returns basic auth
    struct AlwaysBasic;
    impl Keychain for AlwaysBasic {
        fn resolve(&self, _: &str) -> Result<Box<dyn Authenticator>> {
            Ok(Box::new(Basic::new("user".to_string(), "pass".to_string())))
        }
    }

    // Test that MultiKeychain returns the first non-anonymous auth
    let multi = MultiKeychain::new(vec![
        Box::new(AlwaysAnonymous) as Box<dyn Keychain>,
        Box::new(AlwaysBasic) as Box<dyn Keychain>,
    ]);

    let auth = multi.resolve("any.registry.io").unwrap();
    let config = auth.authorization().unwrap();
    assert!(!config.is_anonymous());
    assert_eq!(config.username, Some("user".to_string()));
}
