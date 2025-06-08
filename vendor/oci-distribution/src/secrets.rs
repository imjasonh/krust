//! Types for working with registry access secrets

use crate::errors::Result;
use crate::Reference;

/// A method for authenticating to a registry
#[derive(Eq, PartialEq, Debug, Clone)]
pub enum RegistryAuth {
    /// Access the registry anonymously
    Anonymous,
    /// Access the registry using HTTP Basic authentication
    Basic(String, String),
    /// Access the registry using a Bearer token (OAuth2)
    Bearer(String),
}

impl RegistryAuth {
    /// Create a RegistryAuth by resolving from Docker config files and credential helpers
    ///
    /// This will check:
    /// 1. Docker config files (DOCKER_CONFIG, REGISTRY_AUTH_FILE, ~/.docker/config.json)
    /// 2. Credential helpers specified in the config
    /// 3. Default credential store
    ///
    /// If no credentials are found, returns Anonymous auth.
    pub fn from_default(image: &Reference) -> Result<Self> {
        let image_str = image.whole();
        crate::credential_helper::resolve_docker_auth(&image_str)
    }

    /// Create a RegistryAuth by resolving from Docker config files and credential helpers
    ///
    /// This is similar to `from_default` but takes a string reference instead of a Reference.
    pub fn from_default_str(image_ref: &str) -> Result<Self> {
        crate::credential_helper::resolve_docker_auth(image_ref)
    }
}

pub(crate) trait Authenticable {
    fn apply_authentication(self, auth: &RegistryAuth) -> Self;
}

impl Authenticable for reqwest::RequestBuilder {
    fn apply_authentication(self, auth: &RegistryAuth) -> Self {
        match auth {
            RegistryAuth::Anonymous => self,
            RegistryAuth::Basic(username, password) => self.basic_auth(username, Some(password)),
            RegistryAuth::Bearer(token) => self.bearer_auth(token),
        }
    }
}
