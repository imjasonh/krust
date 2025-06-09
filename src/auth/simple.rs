//! Simple authentication wrapper for registry authentication

use crate::registry::RegistryAuth;
use anyhow::Result;

/// Resolve authentication for a given resource using Docker config and credential helpers
pub fn resolve_auth(resource: &str) -> Result<RegistryAuth> {
    // For now, return anonymous auth
    // TODO: Implement actual credential resolution from Docker config
    let _ = resource; // Silence unused warning
    Ok(RegistryAuth::Anonymous)
}
