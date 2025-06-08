//! Docker credential helper support

use crate::docker_config::{DockerAuthEntry, DockerConfig, extract_registry, load_docker_config, normalize_registry};
use crate::errors::{OciDistributionError, Result};
use crate::secrets::RegistryAuth;
use serde::Deserialize;
use std::io::Write;
use std::process::{Command, Stdio};
use tracing::{debug, warn};

/// Response from a Docker credential helper
#[derive(Deserialize)]
struct HelperResponse {
    #[serde(rename = "Username")]
    username: Option<String>,
    #[serde(rename = "Secret")]
    secret: Option<String>,
    #[serde(rename = "ServerURL")]
    _server_url: Option<String>,
}

/// Execute a credential helper to get credentials
pub fn execute_credential_helper(helper: &str, registry: &str) -> Result<(String, String)> {
    let helper_name = format!("docker-credential-{}", helper);

    debug!("Executing credential helper: {} for {}", helper_name, registry);

    let mut child = Command::new(&helper_name)
        .arg("get")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| OciDistributionError::GenericError(Some(
            format!("Failed to spawn credential helper {}: {}", helper_name, e)
        )))?;

    // Write registry URL to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(registry.as_bytes()).map_err(|e| {
            OciDistributionError::IoError(e)
        })?;
        stdin.write_all(b"\n").map_err(|e| {
            OciDistributionError::IoError(e)
        })?;
    }

    let output = child.wait_with_output().map_err(|e| {
        OciDistributionError::IoError(e)
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OciDistributionError::GenericError(Some(
            format!("Credential helper {} failed: {}", helper_name, stderr)
        )));
    }

    // Parse output as JSON
    let response: HelperResponse = serde_json::from_slice(&output.stdout)
        .map_err(|e| OciDistributionError::GenericError(Some(
            format!("Failed to parse credential helper response: {}", e)
        )))?;

    match (response.username, response.secret) {
        (Some(username), Some(password)) => Ok((username, password)),
        _ => Err(OciDistributionError::GenericError(Some(
            "Credential helper did not return username and password".to_string()
        )))
    }
}

/// Find auth entry for a registry in Docker config
fn find_auth_entry(config: &DockerConfig, registry: &str) -> Option<DockerAuthEntry> {
    let variants = normalize_registry(registry);

    for variant in variants {
        if let Some(entry) = config.auths.get(&variant) {
            return Some(entry.clone());
        }
    }

    None
}

/// Get credential helper for a registry
fn get_credential_helper(config: &DockerConfig, registry: &str) -> Option<String> {
    // Check specific credential helper for registry
    if let Some(helper) = config.cred_helpers.get(registry) {
        return Some(helper.clone());
    }

    // Check default credential store
    config.creds_store.clone()
}

/// Resolve authentication for a given resource using Docker config and credential helpers
pub fn resolve_docker_auth(resource: &str) -> Result<RegistryAuth> {
    let config = load_docker_config()?;
    let registry = extract_registry(resource);

    debug!("Resolving auth for resource: {} (registry: {})", resource, registry);

    // Try to find auth entry in config
    if let Some(auth_entry) = find_auth_entry(&config, registry) {
        debug!("Found auth entry for {}", registry);

        // Check if it's anonymous
        if auth_entry.is_anonymous() {
            return Ok(RegistryAuth::Anonymous);
        }

        // Check for bearer tokens first
        if let Some(token) = auth_entry.registry_token {
            return Ok(RegistryAuth::Bearer(token));
        }

        if let Some(token) = auth_entry.identity_token {
            return Ok(RegistryAuth::Bearer(token));
        }

        // Then check for basic auth
        if let (Some(username), Some(password)) = (&auth_entry.username, &auth_entry.password) {
            return Ok(RegistryAuth::Basic(username.clone(), password.clone()));
        }

        // Try to decode base64 auth string
        if let Some(auth) = &auth_entry.auth {
            use base64::Engine;
            match base64::engine::general_purpose::STANDARD.decode(auth) {
                Ok(decoded) => {
                    if let Ok(decoded_str) = String::from_utf8(decoded) {
                        if let Some((user, pass)) = decoded_str.split_once(':') {
                            return Ok(RegistryAuth::Basic(user.to_string(), pass.to_string()));
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to decode auth for {}: {}", registry, e);
                }
            }
        }
    }

    // Try credential helper
    if let Some(helper) = get_credential_helper(&config, registry) {
        debug!("Trying credential helper: {} for {}", helper, registry);
        match execute_credential_helper(&helper, registry) {
            Ok((username, password)) => {
                return Ok(RegistryAuth::Basic(username, password));
            }
            Err(e) => {
                warn!("Credential helper failed: {}", e);
            }
        }
    }

    // Default to anonymous
    debug!("No credentials found for {}, using anonymous", registry);
    Ok(RegistryAuth::Anonymous)
}

#[cfg(test)]
#[path = "credential_helper_tests.rs"]
mod tests;
