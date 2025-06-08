//! Tests for the auth module

use super::*;

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
