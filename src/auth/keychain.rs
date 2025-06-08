//! Keychain implementation for credential management

use super::{Anonymous, AuthConfig, Authenticator, DockerAuthEntry, DockerConfig};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

/// Trait for types that can resolve authentication for a given resource
pub trait Keychain: Send + Sync {
    /// Resolve authentication for a given resource (registry URL or image reference)
    fn resolve(&self, resource: &str) -> Result<Box<dyn Authenticator>>;
}

/// Default keychain implementation that checks Docker config files
pub struct DefaultKeychain {
    /// Cached config to avoid re-reading files
    config_cache: Arc<Mutex<Option<DockerConfig>>>,
}

impl DefaultKeychain {
    /// Create a new DefaultKeychain
    pub fn new() -> Self {
        Self {
            config_cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Get paths to check for Docker config
    fn config_paths() -> Vec<PathBuf> {
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
    fn load_config(&self) -> Result<DockerConfig> {
        // Check cache first
        {
            let cache = self.config_cache.lock().unwrap();
            if let Some(config) = cache.as_ref() {
                return Ok(config.clone());
            }
        }

        // Try each config path
        for path in Self::config_paths() {
            if path.exists() {
                debug!("Checking Docker config at: {}", path.display());
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        match serde_json::from_str::<DockerConfig>(&content) {
                            Ok(config) => {
                                debug!("Loaded Docker config from: {}", path.display());
                                // Cache the config
                                let mut cache = self.config_cache.lock().unwrap();
                                *cache = Some(config.clone());
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
    fn extract_registry(image_ref: &str) -> &str {
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
    fn normalize_registry(registry: &str) -> Vec<String> {
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

    /// Find auth entry for a registry
    fn find_auth_entry(&self, config: &DockerConfig, registry: &str) -> Option<DockerAuthEntry> {
        let variants = Self::normalize_registry(registry);

        for variant in variants {
            if let Some(entry) = config.auths.get(&variant) {
                return Some(entry.clone());
            }
        }

        None
    }

    /// Get credential helper for a registry
    fn get_credential_helper(&self, config: &DockerConfig, registry: &str) -> Option<String> {
        // Check specific credential helper for registry
        if let Some(helper) = config.cred_helpers.get(registry) {
            return Some(helper.clone());
        }

        // Check default credential store
        config.creds_store.clone()
    }

    /// Execute credential helper to get credentials
    fn execute_credential_helper(&self, helper: &str, registry: &str) -> Result<AuthConfig> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let helper_name = format!("docker-credential-{}", helper);

        debug!(
            "Executing credential helper: {} for {}",
            helper_name, registry
        );

        let mut child = Command::new(&helper_name)
            .arg("get")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!(
                "Failed to spawn credential helper: {}",
                helper_name
            ))?;

        // Write registry URL to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(registry.as_bytes())?;
            stdin.write_all(b"\n")?;
        }

        let output = child.wait_with_output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Credential helper {} failed: {}", helper_name, stderr);
        }

        // Parse output as JSON
        #[derive(serde::Deserialize)]
        struct HelperResponse {
            #[serde(rename = "Username")]
            username: Option<String>,
            #[serde(rename = "Secret")]
            secret: Option<String>,
            #[serde(rename = "ServerURL")]
            _server_url: Option<String>,
        }

        let response: HelperResponse = serde_json::from_slice(&output.stdout)
            .context("Failed to parse credential helper response")?;

        Ok(AuthConfig {
            username: response.username,
            password: response.secret,
            ..Default::default()
        })
    }
}

impl Default for DefaultKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl Keychain for DefaultKeychain {
    fn resolve(&self, resource: &str) -> Result<Box<dyn Authenticator>> {
        let config = self.load_config()?;
        let registry = Self::extract_registry(resource);

        debug!(
            "Resolving auth for resource: {} (registry: {})",
            resource, registry
        );

        // Try to find auth entry in config
        if let Some(auth_entry) = self.find_auth_entry(&config, registry) {
            debug!("Found auth entry for {}", registry);
            let auth_config = auth_entry.to_auth_config();

            // Return appropriate authenticator based on auth type
            if auth_config.is_anonymous() {
                return Ok(Box::new(Anonymous));
            }

            return Ok(Box::new(ConfigAuthenticator {
                config: auth_config,
            }));
        }

        // Try credential helper
        if let Some(helper) = self.get_credential_helper(&config, registry) {
            debug!("Trying credential helper: {} for {}", helper, registry);
            match self.execute_credential_helper(&helper, registry) {
                Ok(auth_config) => {
                    return Ok(Box::new(ConfigAuthenticator {
                        config: auth_config,
                    }));
                }
                Err(e) => {
                    warn!("Credential helper failed: {}", e);
                }
            }
        }

        // Default to anonymous
        debug!("No credentials found for {}, using anonymous", registry);
        Ok(Box::new(Anonymous))
    }
}

/// Authenticator that returns a fixed AuthConfig
struct ConfigAuthenticator {
    config: AuthConfig,
}

impl Authenticator for ConfigAuthenticator {
    fn authorization(&self) -> Result<AuthConfig> {
        Ok(self.config.clone())
    }
}

/// Multi-keychain that tries multiple keychains in order
pub struct MultiKeychain {
    keychains: Vec<Box<dyn Keychain>>,
}

impl MultiKeychain {
    /// Create a new MultiKeychain
    #[allow(dead_code)]
    pub fn new(keychains: Vec<Box<dyn Keychain>>) -> Self {
        Self { keychains }
    }
}

impl Keychain for MultiKeychain {
    fn resolve(&self, resource: &str) -> Result<Box<dyn Authenticator>> {
        for keychain in &self.keychains {
            match keychain.resolve(resource) {
                Ok(auth) => {
                    // Check if it's not anonymous
                    if let Ok(config) = auth.authorization() {
                        if !config.is_anonymous() {
                            return Ok(auth);
                        }
                    }
                }
                Err(e) => {
                    debug!("Keychain failed: {}", e);
                }
            }
        }

        // Default to anonymous
        Ok(Box::new(Anonymous))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_registry() {
        assert_eq!(
            DefaultKeychain::extract_registry("docker.io/library/ubuntu:latest"),
            "docker.io"
        );
        assert_eq!(
            DefaultKeychain::extract_registry("gcr.io/project/image:tag"),
            "gcr.io"
        );
        assert_eq!(
            DefaultKeychain::extract_registry("localhost:5000/image"),
            "localhost:5000"
        );
        assert_eq!(
            DefaultKeychain::extract_registry("ubuntu:latest"),
            "index.docker.io"
        );
        assert_eq!(
            DefaultKeychain::extract_registry("user/image:tag"),
            "index.docker.io"
        );
    }

    #[test]
    fn test_normalize_registry() {
        let variants = DefaultKeychain::normalize_registry("docker.io");
        assert!(variants.contains(&"docker.io".to_string()));
        assert!(variants.contains(&"index.docker.io".to_string()));

        let variants = DefaultKeychain::normalize_registry("gcr.io");
        assert!(variants.contains(&"gcr.io".to_string()));
        assert!(variants.contains(&"https://gcr.io".to_string()));
    }
}
