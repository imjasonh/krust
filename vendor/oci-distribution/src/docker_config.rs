//! Docker config file parsing and credential helper support

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, warn};

/// Docker config file structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DockerConfig {
    /// Registry authentication entries
    #[serde(default)]
    pub auths: HashMap<String, DockerAuthEntry>,
    /// Registry-specific credential helpers
    #[serde(rename = "credHelpers", default)]
    pub cred_helpers: HashMap<String, String>,
    /// Default credential store to use
    #[serde(rename = "credsStore", skip_serializing_if = "Option::is_none")]
    pub creds_store: Option<String>,
}

/// Entry in the Docker config auths section
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DockerAuthEntry {
    /// Base64-encoded username:password
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    /// Username for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Password for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Identity token for registry authentication
    #[serde(rename = "identitytoken", skip_serializing_if = "Option::is_none")]
    pub identity_token: Option<String>,
    /// Registry token for bearer authentication
    #[serde(rename = "registrytoken", skip_serializing_if = "Option::is_none")]
    pub registry_token: Option<String>,
}

impl DockerAuthEntry {
    /// Check if this is anonymous authentication
    pub fn is_anonymous(&self) -> bool {
        self.username.is_none()
            && self.password.is_none()
            && self.auth.is_none()
            && self.identity_token.is_none()
            && self.registry_token.is_none()
    }
}

/// Get paths to check for Docker config
pub fn config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Check DOCKER_CONFIG environment variable
    if let Ok(docker_config) = std::env::var("DOCKER_CONFIG") {
        paths.push(PathBuf::from(docker_config).join("config.json"));
    }

    // Check REGISTRY_AUTH_FILE environment variable
    if let Ok(auth_file) = std::env::var("REGISTRY_AUTH_FILE") {
        paths.push(PathBuf::from(auth_file));
    }

    // Check XDG_RUNTIME_DIR for containers auth
    if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
        paths.push(PathBuf::from(xdg_runtime).join("containers/auth.json"));
    }

    // Check default Docker config location
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".docker/config.json"));
    }

    paths
}

/// Load Docker config from disk
pub fn load_docker_config() -> crate::errors::Result<DockerConfig> {
    // Try each config path
    for path in config_paths() {
        if path.exists() {
            debug!("Checking Docker config at: {}", path.display());
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<DockerConfig>(&content) {
                        Ok(config) => {
                            debug!("Loaded Docker config from: {}", path.display());
                            return Ok(config);
                        }
                        Err(e) => {
                            warn!("Failed to parse Docker config at {}: {}", path.display(), e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read Docker config at {}: {}", path.display(), e);
                }
            }
        }
    }

    // Return empty config if no valid config found
    Ok(DockerConfig {
        auths: HashMap::new(),
        cred_helpers: HashMap::new(),
        creds_store: None,
    })
}

/// Extract registry from image reference
pub fn extract_registry(image_ref: &str) -> &str {
    // Handle different image reference formats:
    // - docker.io/library/ubuntu:latest -> docker.io
    // - gcr.io/project/image:tag -> gcr.io
    // - localhost:5000/image -> localhost:5000
    // - ubuntu:latest -> docker.io (implicit)

    if let Some(slash_pos) = image_ref.find('/') {
        let registry_part = &image_ref[..slash_pos];

        // Check if this looks like a registry (contains . or :)
        if registry_part.contains('.') || registry_part.contains(':') {
            return registry_part;
        }
    }

    // Default to Docker Hub
    "index.docker.io"
}

/// Normalize registry URL for matching
pub fn normalize_registry(registry: &str) -> Vec<String> {
    let mut variants = vec![registry.to_string()];

    // Add common variants
    if registry == "docker.io" || registry == "index.docker.io" {
        variants.push("docker.io".to_string());
        variants.push("index.docker.io".to_string());
        variants.push("https://index.docker.io/v1/".to_string());
        variants.push("https://index.docker.io/v2/".to_string());
    } else if !registry.starts_with("http://") && !registry.starts_with("https://") {
        // Add protocol variants
        variants.push(format!("https://{}", registry));
        variants.push(format!("http://{}", registry));

        // Add /v1/ and /v2/ variants
        variants.push(format!("https://{}/v1/", registry));
        variants.push(format!("https://{}/v2/", registry));
    }

    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_registry() {
        assert_eq!(extract_registry("docker.io/library/ubuntu:latest"), "docker.io");
        assert_eq!(extract_registry("gcr.io/project/image:tag"), "gcr.io");
        assert_eq!(extract_registry("localhost:5000/image"), "localhost:5000");
        assert_eq!(extract_registry("ubuntu:latest"), "index.docker.io");
        assert_eq!(extract_registry("user/image:tag"), "index.docker.io");
    }

    #[test]
    fn test_normalize_registry() {
        let variants = normalize_registry("docker.io");
        assert!(variants.contains(&"docker.io".to_string()));
        assert!(variants.contains(&"index.docker.io".to_string()));

        let variants = normalize_registry("gcr.io");
        assert!(variants.contains(&"gcr.io".to_string()));
        assert!(variants.contains(&"https://gcr.io".to_string()));
    }
}
