//! Simple authentication wrapper using oci-distribution's credential helper

use anyhow::Result;
use oci_distribution::secrets::RegistryAuth;

/// Resolve authentication for a given resource using Docker config and credential helpers
pub fn resolve_auth(resource: &str) -> Result<RegistryAuth> {
    RegistryAuth::from_default_str(resource)
        .map_err(|e| anyhow::anyhow!("Failed to resolve auth: {}", e))
}
