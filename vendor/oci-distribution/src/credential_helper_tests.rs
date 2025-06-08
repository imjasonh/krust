//! Tests for credential helper functionality

#[cfg(test)]
mod tests {
    use crate::credential_helper::*;
    use crate::secrets::RegistryAuth;
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_anonymous_when_no_config() {
        // Create a temp directory for DOCKER_CONFIG that doesn't have any config
        let tmp_dir = TempDir::new().unwrap();

        // Save current env vars
        let old_docker_config = env::var("DOCKER_CONFIG").ok();
        let old_registry_auth = env::var("REGISTRY_AUTH_FILE").ok();

        // Set to our empty temp directory
        env::set_var("DOCKER_CONFIG", tmp_dir.path());
        env::remove_var("REGISTRY_AUTH_FILE");

        let auth = resolve_docker_auth("docker.io/library/alpine").unwrap();
        assert_eq!(auth, RegistryAuth::Anonymous);

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
    }

    #[test]
    fn test_resolve_basic_auth_from_config() {
        let tmp_dir = TempDir::new().unwrap();
        let config_path = tmp_dir.path().join("config.json");

        let config = r#"{
            "auths": {
                "docker.io": {
                    "username": "testuser",
                    "password": "testpass"
                }
            }
        }"#;

        fs::write(&config_path, config).unwrap();

        // Save current env var
        let old_val = env::var("DOCKER_CONFIG").ok();
        env::set_var("DOCKER_CONFIG", tmp_dir.path());

        let auth = resolve_docker_auth("docker.io/library/alpine").unwrap();
        assert_eq!(auth, RegistryAuth::Basic("testuser".to_string(), "testpass".to_string()));

        // Restore env var
        if let Some(val) = old_val {
            env::set_var("DOCKER_CONFIG", val);
        } else {
            env::remove_var("DOCKER_CONFIG");
        }
    }

    #[test]
    fn test_resolve_bearer_token_from_config() {
        let tmp_dir = TempDir::new().unwrap();
        let config_path = tmp_dir.path().join("config.json");

        let config = r#"{
            "auths": {
                "ghcr.io": {
                    "registrytoken": "ghp_abc123token"
                }
            }
        }"#;

        fs::write(&config_path, config).unwrap();

        // Save current env var
        let old_val = env::var("DOCKER_CONFIG").ok();
        env::set_var("DOCKER_CONFIG", tmp_dir.path());

        let auth = resolve_docker_auth("ghcr.io/user/image").unwrap();
        assert_eq!(auth, RegistryAuth::Bearer("ghp_abc123token".to_string()));

        // Restore env var
        if let Some(val) = old_val {
            env::set_var("DOCKER_CONFIG", val);
        } else {
            env::remove_var("DOCKER_CONFIG");
        }
    }

    #[test]
    fn test_resolve_base64_auth_from_config() {
        let tmp_dir = TempDir::new().unwrap();
        let config_path = tmp_dir.path().join("config.json");

        // Base64 encoded "testuser:testpass"
        let config = r#"{
            "auths": {
                "docker.io": {
                    "auth": "dGVzdHVzZXI6dGVzdHBhc3M="
                }
            }
        }"#;

        fs::write(&config_path, config).unwrap();

        // Save current env var
        let old_val = env::var("DOCKER_CONFIG").ok();
        env::set_var("DOCKER_CONFIG", tmp_dir.path());

        let auth = resolve_docker_auth("docker.io/library/alpine").unwrap();
        assert_eq!(auth, RegistryAuth::Basic("testuser".to_string(), "testpass".to_string()));

        // Restore env var
        if let Some(val) = old_val {
            env::set_var("DOCKER_CONFIG", val);
        } else {
            env::remove_var("DOCKER_CONFIG");
        }
    }

    #[test]
    fn test_registry_normalization() {
        let tmp_dir = TempDir::new().unwrap();
        let config_path = tmp_dir.path().join("config.json");

        // Config has index.docker.io but we query with docker.io
        let config = r#"{
            "auths": {
                "index.docker.io": {
                    "username": "testuser",
                    "password": "testpass"
                }
            }
        }"#;

        fs::write(&config_path, config).unwrap();

        // Save current env var
        let old_val = env::var("DOCKER_CONFIG").ok();
        env::set_var("DOCKER_CONFIG", tmp_dir.path());

        // Should find auth even though we use docker.io
        let auth = resolve_docker_auth("docker.io/library/alpine").unwrap();
        assert_eq!(auth, RegistryAuth::Basic("testuser".to_string(), "testpass".to_string()));

        // Restore env var
        if let Some(val) = old_val {
            env::set_var("DOCKER_CONFIG", val);
        } else {
            env::remove_var("DOCKER_CONFIG");
        }
    }

    #[test]
    fn test_credential_helper_mock() {
        let tmp_dir = TempDir::new().unwrap();

        // Create a mock credential helper script
        let helper_path = tmp_dir.path().join("docker-credential-mock");
        let mut file = fs::File::create(&helper_path).unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "echo '{{\"Username\":\"helper-user\",\"Secret\":\"helper-pass\"}}'").unwrap();

        // Make it executable
        let mut perms = fs::metadata(&helper_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&helper_path, perms).unwrap();

        // Create config that uses the helper
        let config_path = tmp_dir.path().join("config.json");
        let config = r#"{
            "credHelpers": {
                "mock.registry.io": "mock"
            }
        }"#;
        fs::write(&config_path, config).unwrap();

        // Save current env vars
        let old_config = env::var("DOCKER_CONFIG").ok();
        let old_path = env::var("PATH").ok();

        // Set our temp dir in PATH and DOCKER_CONFIG
        env::set_var("DOCKER_CONFIG", tmp_dir.path());
        let new_path = format!("{}:{}", tmp_dir.path().display(), env::var("PATH").unwrap_or_default());
        env::set_var("PATH", new_path);

        // Test the credential helper
        let auth = resolve_docker_auth("mock.registry.io/test/image").unwrap();
        assert_eq!(auth, RegistryAuth::Basic("helper-user".to_string(), "helper-pass".to_string()));

        // Restore env vars
        if let Some(val) = old_config {
            env::set_var("DOCKER_CONFIG", val);
        } else {
            env::remove_var("DOCKER_CONFIG");
        }
        if let Some(val) = old_path {
            env::set_var("PATH", val);
        } else {
            env::remove_var("PATH");
        }
    }

    #[test]
    fn test_default_credential_store_mock() {
        let tmp_dir = TempDir::new().unwrap();

        // Create a mock credential helper script
        let helper_path = tmp_dir.path().join("docker-credential-defaultstore");
        let mut file = fs::File::create(&helper_path).unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "echo '{{\"Username\":\"store-user\",\"Secret\":\"store-pass\"}}'").unwrap();

        // Make it executable
        let mut perms = fs::metadata(&helper_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&helper_path, perms).unwrap();

        // Create config that uses default creds store
        let config_path = tmp_dir.path().join("config.json");
        let config = r#"{
            "credsStore": "defaultstore"
        }"#;
        fs::write(&config_path, config).unwrap();

        // Save current env vars
        let old_config = env::var("DOCKER_CONFIG").ok();
        let old_path = env::var("PATH").ok();

        // Set our temp dir in PATH and DOCKER_CONFIG
        env::set_var("DOCKER_CONFIG", tmp_dir.path());
        let new_path = format!("{}:{}", tmp_dir.path().display(), env::var("PATH").unwrap_or_default());
        env::set_var("PATH", new_path);

        // Test the credential helper
        let auth = resolve_docker_auth("any.registry.io/test/image").unwrap();
        assert_eq!(auth, RegistryAuth::Basic("store-user".to_string(), "store-pass".to_string()));

        // Restore env vars
        if let Some(val) = old_config {
            env::set_var("DOCKER_CONFIG", val);
        } else {
            env::remove_var("DOCKER_CONFIG");
        }
        if let Some(val) = old_path {
            env::set_var("PATH", val);
        } else {
            env::remove_var("PATH");
        }
    }

    #[test]
    fn test_registry_auth_file_env() {
        let tmp_dir = TempDir::new().unwrap();
        let auth_file = tmp_dir.path().join("auth.json");

        let config = r#"{
            "auths": {
                "special.registry.io": {
                    "username": "authfile-user",
                    "password": "authfile-pass"
                }
            }
        }"#;

        fs::write(&auth_file, config).unwrap();

        // Save current env var
        let old_val = env::var("REGISTRY_AUTH_FILE").ok();
        env::set_var("REGISTRY_AUTH_FILE", auth_file.to_str().unwrap());

        let auth = resolve_docker_auth("special.registry.io/image").unwrap();
        assert_eq!(auth, RegistryAuth::Basic("authfile-user".to_string(), "authfile-pass".to_string()));

        // Restore env var
        if let Some(val) = old_val {
            env::set_var("REGISTRY_AUTH_FILE", val);
        } else {
            env::remove_var("REGISTRY_AUTH_FILE");
        }
    }
}
