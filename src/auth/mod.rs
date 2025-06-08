//! Authentication module for container registries
//!
//! This module provides authentication functionality similar to go-containerregistry's authn package,
//! supporting Docker config files, credential helpers, and various authentication methods.

use anyhow::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod keychain;

pub use keychain::{DefaultKeychain, Keychain};

/// Authentication configuration containing credentials
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_token: Option<String>,
}

impl AuthConfig {
    /// Create a new AuthConfig with username and password
    pub fn new(username: String, password: String) -> Self {
        Self {
            username: Some(username),
            password: Some(password),
            ..Default::default()
        }
    }

    /// Create an anonymous AuthConfig
    pub fn anonymous() -> Self {
        Self::default()
    }

    /// Check if this is anonymous authentication
    pub fn is_anonymous(&self) -> bool {
        self.username.is_none()
            && self.password.is_none()
            && self.auth.is_none()
            && self.identity_token.is_none()
            && self.registry_token.is_none()
    }

    /// Convert to authorization header value
    pub fn to_authorization_header(&self) -> Result<Option<String>> {
        if let Some(token) = &self.registry_token {
            return Ok(Some(format!("Bearer {}", token)));
        }

        if let Some(token) = &self.identity_token {
            return Ok(Some(format!("Bearer {}", token)));
        }

        if let Some(auth) = &self.auth {
            return Ok(Some(format!("Basic {}", auth)));
        }

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            let encoded = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", username, password));
            return Ok(Some(format!("Basic {}", encoded)));
        }

        Ok(None)
    }

    /// Convert to oci-distribution RegistryAuth
    pub fn to_registry_auth(&self) -> oci_distribution::secrets::RegistryAuth {
        use oci_distribution::secrets::RegistryAuth;

        if self.is_anonymous() {
            return RegistryAuth::Anonymous;
        }

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            return RegistryAuth::Basic(username.clone(), password.clone());
        }

        if let Some(auth) = &self.auth {
            // Try to decode the base64 auth string
            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(auth) {
                if let Ok(decoded_str) = String::from_utf8(decoded) {
                    if let Some((user, pass)) = decoded_str.split_once(':') {
                        return RegistryAuth::Basic(user.to_string(), pass.to_string());
                    }
                }
            }
        }

        RegistryAuth::Anonymous
    }
}

/// Trait for types that can provide authentication
pub trait Authenticator: Send + Sync {
    /// Get the authentication configuration
    fn authorization(&self) -> Result<AuthConfig>;
}

/// Anonymous authenticator
pub struct Anonymous;

impl Authenticator for Anonymous {
    fn authorization(&self) -> Result<AuthConfig> {
        Ok(AuthConfig::anonymous())
    }
}

/// Basic authenticator with username and password
pub struct Basic {
    username: String,
    password: String,
}

impl Basic {
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

impl Authenticator for Basic {
    fn authorization(&self) -> Result<AuthConfig> {
        Ok(AuthConfig::new(
            self.username.clone(),
            self.password.clone(),
        ))
    }
}

/// Bearer token authenticator
pub struct Bearer {
    token: String,
}

impl Bearer {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}

impl Authenticator for Bearer {
    fn authorization(&self) -> Result<AuthConfig> {
        Ok(AuthConfig {
            registry_token: Some(self.token.clone()),
            ..Default::default()
        })
    }
}

/// Docker config file structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DockerConfig {
    #[serde(default)]
    pub auths: HashMap<String, DockerAuthEntry>,
    #[serde(rename = "credHelpers", default)]
    pub cred_helpers: HashMap<String, String>,
    #[serde(rename = "credsStore", skip_serializing_if = "Option::is_none")]
    pub creds_store: Option<String>,
}

/// Entry in the Docker config auths section
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DockerAuthEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(rename = "identitytoken", skip_serializing_if = "Option::is_none")]
    pub identity_token: Option<String>,
    #[serde(rename = "registrytoken", skip_serializing_if = "Option::is_none")]
    pub registry_token: Option<String>,
}

impl DockerAuthEntry {
    /// Convert to AuthConfig
    pub fn to_auth_config(&self) -> AuthConfig {
        AuthConfig {
            username: self.username.clone(),
            password: self.password.clone(),
            auth: self.auth.clone(),
            identity_token: self.identity_token.clone(),
            registry_token: self.registry_token.clone(),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_auth_config_anonymous() {
        let auth = AuthConfig::anonymous();
        assert!(auth.is_anonymous());
        assert_eq!(auth.to_authorization_header().unwrap(), None);
    }

    #[test]
    fn test_auth_config_basic() {
        let auth = AuthConfig::new("user".to_string(), "pass".to_string());
        assert!(!auth.is_anonymous());

        let header = auth.to_authorization_header().unwrap().unwrap();
        assert!(header.starts_with("Basic "));

        // Verify base64 encoding
        let expected = base64::engine::general_purpose::STANDARD.encode("user:pass");
        assert_eq!(header, format!("Basic {}", expected));
    }

    #[test]
    fn test_auth_config_bearer() {
        let auth = AuthConfig {
            registry_token: Some("token123".to_string()),
            ..Default::default()
        };

        let header = auth.to_authorization_header().unwrap().unwrap();
        assert_eq!(header, "Bearer token123");
    }
}
