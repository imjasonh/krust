//! Simple authentication wrapper for registry authentication

use crate::registry::{ImageReference, RegistryAuth};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tracing::debug;

use super::{DockerAuthEntry, DockerConfig};

/// Resolve authentication for a given resource using Docker config and credential helpers
pub fn resolve_auth(resource: &str) -> Result<RegistryAuth> {
    debug!("Resolving auth for resource: {}", resource);

    // Parse the resource to extract registry
    let registry = if let Ok(image_ref) = ImageReference::parse(resource) {
        image_ref.registry
    } else if resource.contains('/') {
        // If it looks like a repository (registry/repo), extract registry part
        resource.split('/').next().unwrap_or(resource).to_string()
    } else {
        // Just use the resource as-is (might be a registry hostname)
        resource.to_string()
    };

    debug!("Extracted registry from resource: {}", registry);

    // Try to read Docker config
    if let Ok(auth) = read_docker_config(&registry) {
        debug!("Found auth in Docker config for registry: {}", registry);
        return Ok(auth);
    }

    // Try credential helpers
    if let Ok(auth) = try_credential_helpers(&registry) {
        debug!(
            "Found auth via credential helper for registry: {}",
            registry
        );
        return Ok(auth);
    }

    debug!("No auth found, using anonymous for registry: {}", registry);
    Ok(RegistryAuth::Anonymous)
}

fn read_docker_config(registry: &str) -> Result<RegistryAuth> {
    let config_paths = get_docker_config_paths();

    for config_path in config_paths {
        if let Ok(config_content) = fs::read_to_string(&config_path) {
            debug!("Reading Docker config from: {:?}", config_path);

            if let Ok(config) = serde_json::from_str::<DockerConfig>(&config_content) {
                if let Some(auths) = &config.auths {
                    // Try exact registry match first
                    if let Some(auth_entry) = auths.get(registry) {
                        debug!("Found exact registry match for: {}", registry);
                        return parse_auth_entry(auth_entry);
                    }

                    // Try with https:// prefix (common in Docker config)
                    let https_registry = format!("https://{}", registry);
                    if let Some(auth_entry) = auths.get(&https_registry) {
                        debug!("Found https registry match for: {}", https_registry);
                        return parse_auth_entry(auth_entry);
                    }

                    // Try registry-1.docker.io for docker.io
                    if registry == "docker.io" || registry == "registry-1.docker.io" {
                        for key in &[
                            "docker.io",
                            "registry-1.docker.io",
                            "https://index.docker.io/v1/",
                        ] {
                            if let Some(auth_entry) = auths.get(*key) {
                                debug!("Found Docker Hub match with key: {}", key);
                                return parse_auth_entry(auth_entry);
                            }
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!("No auth found in Docker config")
}

fn parse_auth_entry(auth_entry: &DockerAuthEntry) -> Result<RegistryAuth> {
    // Check for bearer token first
    if let Some(token) = &auth_entry.registry_token {
        debug!("Using bearer token auth");
        return Ok(RegistryAuth::Bearer {
            token: token.clone(),
        });
    }

    // Check for basic auth credentials
    if let (Some(username), Some(password)) = (&auth_entry.username, &auth_entry.password) {
        debug!("Using basic auth with username/password");
        return Ok(RegistryAuth::Basic {
            username: username.clone(),
            password: password.clone(),
        });
    }

    // Check for base64 encoded auth
    if let Some(auth_b64) = &auth_entry.auth {
        debug!("Using base64 encoded auth");
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(auth_b64)
            .context("Failed to decode base64 auth")?;
        let auth_str = String::from_utf8(decoded).context("Auth is not valid UTF-8")?;

        if let Some((username, password)) = auth_str.split_once(':') {
            return Ok(RegistryAuth::Basic {
                username: username.to_string(),
                password: password.to_string(),
            });
        }
    }

    anyhow::bail!("No valid auth found in auth entry")
}

fn get_docker_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Check DOCKER_CONFIG environment variable
    if let Ok(docker_config) = std::env::var("DOCKER_CONFIG") {
        paths.push(PathBuf::from(docker_config).join("config.json"));
    }

    // Check HOME/.docker/config.json
    if let Ok(home) = std::env::var("HOME") {
        paths.push(PathBuf::from(home).join(".docker").join("config.json"));
    }

    // Check XDG_RUNTIME_DIR for rootless Docker
    if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
        paths.push(
            PathBuf::from(xdg_runtime)
                .join("containers")
                .join("auth.json"),
        );
    }

    paths
}

fn try_credential_helpers(registry: &str) -> Result<RegistryAuth> {
    let config_paths = get_docker_config_paths();

    for config_path in config_paths {
        if let Ok(config_content) = fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str::<DockerConfig>(&config_content) {
                // Check specific credential helpers first
                if let Some(cred_helpers) = &config.cred_helpers {
                    if let Some(helper) = cred_helpers.get(registry) {
                        debug!(
                            "Trying credential helper '{}' for registry: {}",
                            helper, registry
                        );
                        if let Ok(auth) = call_credential_helper(helper, registry) {
                            return Ok(auth);
                        }
                    }
                }

                // Try default credential store
                if let Some(helper) = &config.creds_store {
                    debug!(
                        "Trying default credential helper '{}' for registry: {}",
                        helper, registry
                    );
                    if let Ok(auth) = call_credential_helper(helper, registry) {
                        return Ok(auth);
                    }
                }
            }
        }
    }

    anyhow::bail!("No credential helpers found")
}

#[derive(Debug, Deserialize)]
struct CredentialHelperResponse {
    #[serde(rename = "Username")]
    username: String,
    #[serde(rename = "Secret")]
    secret: String,
}

fn call_credential_helper(helper: &str, registry: &str) -> Result<RegistryAuth> {
    use std::io::Write;
    use std::process::Stdio;

    let helper_name = format!("docker-credential-{}", helper);

    debug!("Calling credential helper: {}", helper_name);

    let mut child = Command::new(&helper_name)
        .arg("get")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(format!(
            "Failed to execute credential helper: {}",
            helper_name
        ))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(registry.as_bytes())
            .context("Failed to write to credential helper stdin")?;
    }

    let output = child
        .wait_with_output()
        .context("Failed to wait for credential helper")?;

    if !output.status.success() {
        anyhow::bail!(
            "Credential helper {} failed: {}",
            helper_name,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let response: CredentialHelperResponse = serde_json::from_slice(&output.stdout)
        .context("Failed to parse credential helper response")?;

    Ok(RegistryAuth::Basic {
        username: response.username,
        password: response.secret,
    })
}
