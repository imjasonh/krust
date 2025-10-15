use anyhow::{Context, Result};
use base64::Engine;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

// OCI Manifest and descriptor types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciImageManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<OciDescriptor>,
    pub layers: Vec<OciDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageIndexEntry {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub digest: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciImageIndex {
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub manifests: Vec<ImageIndexEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

// Authentication structures
#[derive(Debug, Clone)]
pub enum RegistryAuth {
    Anonymous,
    Basic { username: String, password: String },
    Bearer { token: String },
}

#[derive(Debug, Deserialize)]
struct AuthChallenge {
    realm: String,
    service: String,
    scope: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
    #[serde(default)]
    access_token: String,
}

// Image reference parsing
#[derive(Debug, Clone)]
pub struct ImageReference {
    pub registry: String,
    pub repository: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
}

impl ImageReference {
    pub fn parse(reference: &str) -> Result<Self> {
        let reference = reference.trim();

        // Split on @ for digest
        let (repo_part, digest) = if let Some(at_pos) = reference.rfind('@') {
            let digest = reference[at_pos + 1..].to_string();
            let repo_part = &reference[..at_pos];
            (repo_part, Some(digest))
        } else {
            (reference, None)
        };

        // Split on : for tag (but not if there's a digest)
        let (repo_part, tag) = if digest.is_none() {
            if let Some(colon_pos) = repo_part.rfind(':') {
                // Check if this might be a port number instead of a tag
                // A port number would only appear in the registry part (before any '/')
                let potential_tag = &repo_part[colon_pos + 1..];
                let part_before_colon = &repo_part[..colon_pos];

                // Only treat as port if there's no '/' after the colon and it's all digits
                if potential_tag.chars().all(|c| c.is_ascii_digit())
                    && !part_before_colon.contains('/')
                    && colon_pos > 0
                {
                    // This looks like a port number in registry, treat as no tag
                    (repo_part, None)
                } else {
                    let tag = potential_tag.to_string();
                    let repo_part = &repo_part[..colon_pos];
                    (repo_part, Some(tag))
                }
            } else {
                (repo_part, None)
            }
        } else {
            (repo_part, None)
        };

        // Split registry from repository
        let parts: Vec<&str> = repo_part.split('/').collect();
        let (registry, repository) = if parts.len() == 1 {
            // No explicit registry, assume registry-1.docker.io (Docker Hub)
            (
                "registry-1.docker.io".to_string(),
                format!("library/{}", parts[0]),
            )
        } else if parts[0].contains('.') || parts[0].contains(':') || parts[0] == "localhost" {
            // First part looks like a registry
            let registry = parts[0].to_string();
            // Handle docker.io redirect
            let registry = if registry == "docker.io" {
                "registry-1.docker.io".to_string()
            } else {
                registry
            };
            let repository = parts[1..].join("/");
            (registry, repository)
        } else {
            // No explicit registry, assume registry-1.docker.io (Docker Hub)
            ("registry-1.docker.io".to_string(), repo_part.to_string())
        };

        Ok(ImageReference {
            registry,
            repository,
            tag,
            digest,
        })
    }

    pub fn reference(&self) -> String {
        if let Some(digest) = &self.digest {
            format!("{}@{}", self.repository_url(), digest)
        } else {
            format!(
                "{}:{}",
                self.repository_url(),
                self.tag.as_deref().unwrap_or("latest")
            )
        }
    }

    pub fn repository_url(&self) -> String {
        format!("{}/{}", self.registry, self.repository)
    }
}

pub struct RegistryClient {
    client: reqwest::Client,
    #[allow(dead_code)]
    auth_cache: HashMap<String, String>, // registry -> token
}

impl RegistryClient {
    pub fn new() -> Result<Self> {
        // Create two clients - one with redirects, one without
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            auth_cache: HashMap::new(),
        })
    }

    /// Check if a blob exists in the registry using HEAD request
    async fn blob_exists(
        &mut self,
        registry: &str,
        repository: &str,
        digest: &str,
        auth: &RegistryAuth,
    ) -> Result<bool> {
        let url = format!("https://{}/v2/{}/blobs/{}", registry, repository, digest);

        let token = self.authenticate(registry, repository, auth).await?;

        let mut req = self.client.head(&url);

        if let Some(token) = token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let response = req.send().await?;

        Ok(response.status().is_success())
    }

    /// Check if a manifest exists in the registry using HEAD request
    async fn manifest_exists(
        &mut self,
        registry: &str,
        repository: &str,
        digest: &str,
        auth: &RegistryAuth,
    ) -> Result<bool> {
        let url = format!(
            "https://{}/v2/{}/manifests/{}",
            registry, repository, digest
        );

        let token = self.authenticate(registry, repository, auth).await?;

        let mut req = self.client
            .head(&url)
            .header(
                "Accept",
                "application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json",
            );

        if let Some(token) = token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let response = req.send().await?;

        Ok(response.status().is_success())
    }

    // Authenticate with registry and get bearer token if needed
    async fn authenticate(
        &mut self,
        registry: &str,
        repository: &str,
        auth: &RegistryAuth,
    ) -> Result<Option<String>> {
        match auth {
            RegistryAuth::Anonymous => {
                // Try to get anonymous token for the scope
                self.get_anonymous_token(registry, repository).await
            }
            RegistryAuth::Basic { username, password } => {
                // Check if this is actually an OAuth token disguised as basic auth
                // GCR/GAR credential helpers return username like "_dcgcloud_token" or "oauth2accesstoken"
                // with the password being an OAuth token
                if username.starts_with("_") || username == "oauth2accesstoken" {
                    // Treat the password as a bearer token
                    Ok(Some(password.clone()))
                } else {
                    // Use basic auth directly or get token
                    self.get_token_with_basic_auth(registry, repository, username, password)
                        .await
                }
            }
            RegistryAuth::Bearer { token } => Ok(Some(token.clone())),
        }
    }

    async fn get_anonymous_token(
        &mut self,
        registry: &str,
        repository: &str,
    ) -> Result<Option<String>> {
        // First check API support
        let check_url = format!("https://{}/v2/", registry);
        let response = self.client.get(&check_url).send().await?;

        if response.status() == StatusCode::UNAUTHORIZED {
            if let Some(www_auth) = response.headers().get("www-authenticate") {
                let auth_header = www_auth.to_str()?;
                if let Some(challenge) = self.parse_auth_challenge(auth_header)? {
                    return self.request_anonymous_token(&challenge, repository).await;
                }
            }
        }

        Ok(None)
    }

    async fn get_token_with_basic_auth(
        &mut self,
        registry: &str,
        repository: &str,
        username: &str,
        password: &str,
    ) -> Result<Option<String>> {
        // Similar to anonymous but with basic auth
        let check_url = format!("https://{}/v2/", registry);
        let auth_header = format!("{}:{}", username, password);
        let encoded_auth = base64::engine::general_purpose::STANDARD.encode(auth_header.as_bytes());

        let response = self
            .client
            .get(&check_url)
            .header("Authorization", format!("Basic {}", encoded_auth))
            .send()
            .await?;

        if response.status() == StatusCode::UNAUTHORIZED {
            if let Some(www_auth) = response.headers().get("www-authenticate") {
                let auth_header = www_auth.to_str()?;
                if let Some(challenge) = self.parse_auth_challenge(auth_header)? {
                    return self
                        .request_token_with_basic(&challenge, repository, username, password)
                        .await;
                }
            }
        }

        Ok(None)
    }

    fn parse_auth_challenge(&self, auth_header: &str) -> Result<Option<AuthChallenge>> {
        if !auth_header.starts_with("Bearer ") {
            return Ok(None);
        }

        let params_str = &auth_header[7..]; // Remove "Bearer "
        let mut realm = String::new();
        let mut service = String::new();
        let mut scope = String::new();

        for part in params_str.split(',') {
            let part = part.trim();
            if let Some(eq_pos) = part.find('=') {
                let key = part[..eq_pos].trim();
                let value = part[eq_pos + 1..].trim().trim_matches('"');

                match key {
                    "realm" => realm = value.to_string(),
                    "service" => service = value.to_string(),
                    "scope" => scope = value.to_string(),
                    _ => {}
                }
            }
        }

        if !realm.is_empty() {
            Ok(Some(AuthChallenge {
                realm,
                service,
                scope,
            }))
        } else {
            Ok(None)
        }
    }

    async fn request_anonymous_token(
        &mut self,
        challenge: &AuthChallenge,
        repository: &str,
    ) -> Result<Option<String>> {
        let scope = if challenge.scope.is_empty() {
            format!("repository:{}:pull,push", repository)
        } else {
            challenge.scope.clone()
        };

        let token_url = format!(
            "{}?service={}&scope={}",
            challenge.realm, challenge.service, scope
        );

        let response = self.client.get(&token_url).send().await?;

        if response.status().is_success() {
            let token_response: TokenResponse = response.json().await?;
            let token = if !token_response.token.is_empty() {
                token_response.token
            } else {
                token_response.access_token
            };
            Ok(Some(token))
        } else {
            Ok(None)
        }
    }

    async fn request_token_with_basic(
        &mut self,
        challenge: &AuthChallenge,
        repository: &str,
        username: &str,
        password: &str,
    ) -> Result<Option<String>> {
        let scope = if challenge.scope.is_empty() {
            format!("repository:{}:pull,push", repository)
        } else {
            challenge.scope.clone()
        };

        let token_url = format!(
            "{}?service={}&scope={}",
            challenge.realm, challenge.service, scope
        );
        let auth_header = format!("{}:{}", username, password);
        let encoded_auth = base64::engine::general_purpose::STANDARD.encode(auth_header.as_bytes());

        let response = self
            .client
            .get(&token_url)
            .header("Authorization", format!("Basic {}", encoded_auth))
            .send()
            .await?;

        if response.status().is_success() {
            let token_response: TokenResponse = response.json().await?;
            let token = if !token_response.token.is_empty() {
                token_response.token
            } else {
                token_response.access_token
            };
            Ok(Some(token))
        } else {
            Ok(None)
        }
    }

    // Pull a manifest from the registry
    pub async fn pull_manifest(
        &mut self,
        image_ref: &str,
        auth: &RegistryAuth,
    ) -> Result<(OciImageManifest, String)> {
        debug!("Parsing image reference: {}", image_ref);
        let reference = ImageReference::parse(image_ref)?;
        debug!(
            "Parsed reference: registry={}, repository={}, tag={:?}, digest={:?}",
            reference.registry, reference.repository, reference.tag, reference.digest
        );
        let token = self
            .authenticate(&reference.registry, &reference.repository, auth)
            .await?;

        let manifest_ref = if let Some(digest) = &reference.digest {
            digest.clone()
        } else {
            reference.tag.as_deref().unwrap_or("latest").to_string()
        };

        let url = format!(
            "https://{}/v2/{}/manifests/{}",
            reference.registry, reference.repository, manifest_ref
        );

        debug!("Pulling manifest from URL: {}", url);

        let mut req = self.client
            .get(&url)
            .header("Accept", "application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json");

        if let Some(token) = token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to pull manifest: {}", response.status());
        }

        let digest = response
            .headers()
            .get("docker-content-digest")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = response.bytes().await?;
        debug!("Manifest response body: {}", String::from_utf8_lossy(&body));

        // Try to parse as either image manifest or image index
        let manifest: OciImageManifest = if let Ok(image_manifest) =
            serde_json::from_slice::<OciImageManifest>(&body)
        {
            image_manifest
        } else if let Ok(image_index) = serde_json::from_slice::<OciImageIndex>(&body) {
            // If it's an image index, we need to find the specific platform manifest
            // For now, just take the first one (this should be enhanced to match platform)
            if let Some(first_manifest) = image_index.manifests.first() {
                // Pull the platform-specific manifest directly
                let platform_digest = &first_manifest.digest;
                let url = format!(
                    "https://{}/v2/{}/manifests/{}",
                    reference.registry, reference.repository, platform_digest
                );

                debug!("Pulling platform-specific manifest from URL: {}", url);

                let mut req = self.client
                    .get(&url)
                    .header("Accept", "application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json");

                // Re-authenticate for the platform-specific request
                let platform_token = self
                    .authenticate(&reference.registry, &reference.repository, auth)
                    .await?;
                if let Some(token) = platform_token {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }

                let response = req.send().await?;

                if !response.status().is_success() {
                    anyhow::bail!("Failed to pull platform manifest: {}", response.status());
                }

                let platform_body = response.bytes().await?;
                debug!(
                    "Platform manifest response body: {}",
                    String::from_utf8_lossy(&platform_body)
                );

                serde_json::from_slice::<OciImageManifest>(&platform_body)?
            } else {
                anyhow::bail!("Image index has no manifests");
            }
        } else {
            anyhow::bail!("Response is neither a valid image manifest nor image index");
        };

        Ok((manifest, digest))
    }

    // Pull a blob from the registry
    pub async fn pull_blob(
        &mut self,
        image_ref: &str,
        descriptor: &OciDescriptor,
        auth: &RegistryAuth,
    ) -> Result<Vec<u8>> {
        let reference = ImageReference::parse(image_ref)?;
        let token = self
            .authenticate(&reference.registry, &reference.repository, auth)
            .await?;

        let url = format!(
            "https://{}/v2/{}/blobs/{}",
            reference.registry, reference.repository, descriptor.digest
        );

        let mut req = self.client.get(&url);

        if let Some(token) = token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let response = req.send().await?;

        // Handle redirects manually (since we disabled automatic redirects)
        if response.status().is_redirection() {
            if let Some(location) = response.headers().get("location") {
                let redirect_url = location.to_str()?;
                debug!("Following blob download redirect to: {}", redirect_url);
                // Don't include auth header for redirects (might be to CDN/GCS)
                let redirect_response = self.client.get(redirect_url).send().await?;
                if !redirect_response.status().is_success() {
                    anyhow::bail!(
                        "Failed to pull blob {} from redirect: {}",
                        descriptor.digest,
                        redirect_response.status()
                    );
                }
                let body = redirect_response.bytes().await?;
                return Ok(body.to_vec());
            }
        }

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to pull blob {}: {}",
                descriptor.digest,
                response.status()
            );
        }

        let body = response.bytes().await?;
        Ok(body.to_vec())
    }

    // Push a blob to the registry
    pub async fn push_blob(
        &mut self,
        image_ref: &str,
        data: &[u8],
        digest: &str,
        auth: &RegistryAuth,
    ) -> Result<()> {
        let reference = ImageReference::parse(image_ref)?;

        // Check if blob already exists
        if self
            .blob_exists(&reference.registry, &reference.repository, digest, auth)
            .await?
        {
            debug!("Blob {} already exists, skipping push", digest);
            return Ok(());
        }

        info!("Pushing blob: {} to {}", digest, image_ref);
        let token = self
            .authenticate(&reference.registry, &reference.repository, auth)
            .await?;

        // Start upload
        let upload_url = format!(
            "https://{}/v2/{}/blobs/uploads/",
            reference.registry, reference.repository
        );

        let mut req = self.client.post(&upload_url).header("Content-Length", "0");

        if let Some(token) = &token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to start blob upload: {}", response.status());
        }

        let location = response
            .headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .context("No location header in upload response")?;

        debug!("Upload location header: {}", location);

        // Try monolithic upload (PUT with body and ?digest=)
        // If GAR redirects, it means it wants resumable upload instead
        let put_url = if location.starts_with("http") {
            if location.contains('?') {
                format!("{}&digest={}", location, digest)
            } else {
                format!("{}?digest={}", location, digest)
            }
        } else if location.starts_with("/v2/") {
            if location.contains('?') {
                format!(
                    "https://{}{}&digest={}",
                    reference.registry, location, digest
                )
            } else {
                format!(
                    "https://{}{}?digest={}",
                    reference.registry, location, digest
                )
            }
        } else {
            format!(
                "https://{}/v2/{}/blobs/uploads/{}?digest={}",
                reference.registry, reference.repository, location, digest
            )
        };

        debug!("Uploading blob to: {}", &put_url[..100.min(put_url.len())]);

        // Try monolithic upload first
        let mut monolithic_req = self
            .client
            .put(&put_url)
            .header("Content-Type", "application/octet-stream")
            .body(data.to_vec());

        if let Some(ref token_str) = token {
            monolithic_req =
                monolithic_req.header("Authorization", format!("Bearer {}", token_str));
        }

        let monolithic_response = monolithic_req.send().await?;
        let monolithic_status = monolithic_response.status();

        // If monolithic upload succeeds, we're done
        if monolithic_status.is_success() {
            return Ok(());
        }

        // If we get a redirect, GAR wants resumable upload
        // Don't follow the redirect - just use resumable flow
        if monolithic_status.is_redirection() {
            // Build upload location without digest for PATCH
            let upload_location = if location.starts_with("http") {
                location.to_string()
            } else if location.starts_with("/") {
                // Relative URL starting with / (handles /v2/... and /artifacts-uploads/...)
                format!("https://{}{}", reference.registry, location)
            } else {
                // Just a UUID
                format!(
                    "https://{}/v2/{}/blobs/uploads/{}",
                    reference.registry, reference.repository, location
                )
            };

            // PATCH to upload data (don't follow redirects manually)
            let mut patch_req = self
                .client
                .patch(&upload_location)
                .header("Content-Type", "application/octet-stream")
                .body(data.to_vec());

            if let Some(ref token_str) = token {
                patch_req = patch_req.header("Authorization", format!("Bearer {}", token_str));
            }

            let patch_response = patch_req.send().await?;
            let patch_status = patch_response.status();
            let patch_headers = patch_response.headers().clone();

            // PATCH might also return 301 redirect - treat as success if so
            let finalize_location = if patch_status.is_redirection() {
                patch_headers
                    .get("location")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or(location)
            } else if patch_status.is_success() {
                // Get location from successful PATCH response
                patch_headers
                    .get("location")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or(location)
            } else {
                let body = patch_response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to PATCH blob: {} - {}", patch_status, body);
            };

            // Build finalize URL with digest
            let finalize_url = if finalize_location.starts_with("http") {
                if finalize_location.contains('?') {
                    format!("{}&digest={}", finalize_location, digest)
                } else {
                    format!("{}?digest={}", finalize_location, digest)
                }
            } else if finalize_location.starts_with("/") {
                // Relative URL starting with / (handles /v2/... and /artifacts-uploads/...)
                if finalize_location.contains('?') {
                    format!(
                        "https://{}{}&digest={}",
                        reference.registry, finalize_location, digest
                    )
                } else {
                    format!(
                        "https://{}{}?digest={}",
                        reference.registry, finalize_location, digest
                    )
                }
            } else {
                // Just a UUID
                format!(
                    "https://{}/v2/{}/blobs/uploads/{}?digest={}",
                    reference.registry, reference.repository, finalize_location, digest
                )
            };

            // PUT to finalize
            let mut finalize_req = self.client.put(&finalize_url).header("Content-Length", "0");

            if let Some(ref token_str) = token {
                finalize_req =
                    finalize_req.header("Authorization", format!("Bearer {}", token_str));
            }

            let finalize_response = finalize_req.send().await?;
            let finalize_status = finalize_response.status();

            if !finalize_status.is_success() {
                let body = finalize_response.text().await.unwrap_or_default();
                anyhow::bail!("Failed to finalize: {} - {}", finalize_status, body);
            }

            return Ok(());
        }

        // If not success or redirect, fail
        let body = monolithic_response.text().await.unwrap_or_default();
        anyhow::bail!("Failed to upload blob: {} - {}", monolithic_status, body)
    }

    // Push a manifest to the registry
    pub async fn push_manifest(
        &mut self,
        image_ref: &str,
        manifest: &OciImageManifest,
        auth: &RegistryAuth,
    ) -> Result<(String, String)> {
        let reference = ImageReference::parse(image_ref)?;
        let manifest_json = serde_json::to_vec_pretty(manifest)?;
        let manifest_digest = format!("sha256:{}", sha256::digest(&manifest_json));

        // Check if manifest already exists
        if self
            .manifest_exists(
                &reference.registry,
                &reference.repository,
                &manifest_digest,
                auth,
            )
            .await?
        {
            debug!("Manifest {} already exists, skipping push", manifest_digest);
            let digest_ref = format!(
                "{}/{}@{}",
                reference.registry, reference.repository, manifest_digest
            );
            return Ok((digest_ref, manifest_digest));
        }

        info!("Pushing manifest with digest: {}", manifest_digest);

        let token = self
            .authenticate(&reference.registry, &reference.repository, auth)
            .await?;

        // Use tag if provided, otherwise push by digest
        let manifest_ref = reference.tag.as_deref().unwrap_or(&manifest_digest);
        let url = format!(
            "https://{}/v2/{}/manifests/{}",
            reference.registry, reference.repository, manifest_ref
        );

        info!("Pushing manifest to: {}", url);

        let mut req = self
            .client
            .put(&url)
            .header("Content-Type", &manifest.media_type)
            .body(manifest_json.clone());

        if let Some(token) = &token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let response = req.send().await?;
        let status = response.status();
        let headers = response.headers().clone();

        if !status.is_success() {
            let body_str = response.text().await.unwrap_or_default();
            anyhow::bail!("Failed to push manifest: {} - {}", status, body_str);
        }

        let digest = headers
            .get("docker-content-digest")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        let location = headers
            .get("location")
            .and_then(|h| h.to_str().ok())
            .unwrap_or(&url)
            .to_string();

        Ok((location, digest))
    }

    // Legacy methods for compatibility with existing code
    pub async fn push_image_by_digest(
        &mut self,
        repository: &str,
        config_data: Vec<u8>,
        layers: Vec<(Vec<u8>, String)>,
        auth: &RegistryAuth,
    ) -> Result<(String, usize)> {
        // Push config blob
        let config_digest = format!("sha256:{}", sha256::digest(&config_data));
        debug!("Pushing config blob: {}", config_digest);
        self.push_blob(repository, &config_data, &config_digest, auth)
            .await?;

        // Push layers and build manifest
        let mut manifest_layers = Vec::new();
        for (layer_data, media_type) in layers {
            let digest = format!("sha256:{}", sha256::digest(&layer_data));
            debug!("Pushing layer: {}", digest);
            self.push_blob(repository, &layer_data, &digest, auth)
                .await?;

            manifest_layers.push(OciDescriptor {
                media_type: media_type.clone(),
                digest: digest.clone(),
                size: layer_data.len() as i64,
                urls: None,
                annotations: None,
            });
        }

        // Create and push manifest
        let manifest = OciImageManifest {
            schema_version: 2,
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            config: Some(OciDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: config_digest,
                size: config_data.len() as i64,
                urls: None,
                annotations: None,
            }),
            layers: manifest_layers,
            annotations: None,
        };

        let (_, digest) = self.push_manifest(repository, &manifest, auth).await?;
        let reference = ImageReference::parse(repository)?;
        let digest_ref = format!("{}/{}@{}", reference.registry, reference.repository, digest);
        let manifest_size = serde_json::to_vec(&manifest)?.len();

        Ok((digest_ref, manifest_size))
    }

    pub async fn fetch_image_data(
        &mut self,
        image_ref: &str,
        _platform: &str,
        auth: &RegistryAuth,
    ) -> Result<(OciImageManifest, crate::image::ImageConfig)> {
        let (manifest, _digest) = self.pull_manifest(image_ref, auth).await?;

        if let Some(config_descriptor) = &manifest.config {
            let config_data = self.pull_blob(image_ref, config_descriptor, auth).await?;
            let config: crate::image::ImageConfig = serde_json::from_slice(&config_data)?;
            Ok((manifest, config))
        } else {
            anyhow::bail!("Manifest has no config descriptor");
        }
    }

    pub async fn get_image_platforms(
        &mut self,
        _image_ref: &str,
        _auth: &RegistryAuth,
    ) -> Result<Vec<String>> {
        // For now, return default platforms - this would need to be enhanced
        // to actually fetch and parse image indexes
        Ok(vec!["linux/amd64".to_string(), "linux/arm64".to_string()])
    }

    /// Push a layered image where only the top layer is new
    #[allow(clippy::too_many_arguments)]
    pub async fn push_layered_image(
        &mut self,
        repository: &str,
        config_data: Vec<u8>,
        new_layer_data: Vec<u8>,
        _new_layer_media_type: String,
        manifest: &crate::image::Manifest,
        auth: &RegistryAuth,
        base_image_ref: &str,
        base_auth: &RegistryAuth,
    ) -> Result<(String, usize)> {
        // Push config blob
        let config_digest = format!("sha256:{}", sha256::digest(&config_data));
        self.push_blob(repository, &config_data, &config_digest, auth)
            .await?;

        // Copy base image layers if they don't exist in target registry
        let base_reference = ImageReference::parse(base_image_ref)?;
        let target_reference = ImageReference::parse(repository)?;

        // Check if we need to copy base layers (cross-registry scenario)
        let need_copy_layers = base_reference.registry != target_reference.registry;

        if need_copy_layers {
            info!(
                "Copying base image layers from {} to {}",
                base_reference.registry, target_reference.registry
            );

            // Create a separate client for the base registry
            let mut base_client = RegistryClient::new()?;

            // Copy each base layer (all except the last one which is our app layer)
            for layer in &manifest.layers[..manifest.layers.len().saturating_sub(1)] {
                debug!("Copying base layer: {}", layer.digest);

                // Create OciDescriptor for compatibility
                let layer_descriptor = OciDescriptor {
                    media_type: layer.media_type.clone(),
                    digest: layer.digest.clone(),
                    size: layer.size,
                    urls: None,
                    annotations: None,
                };

                // Pull the layer from base registry
                let layer_data = base_client
                    .pull_blob(base_image_ref, &layer_descriptor, base_auth)
                    .await?;

                // Push the layer to target registry
                self.push_blob(repository, &layer_data, &layer.digest, auth)
                    .await?;
            }
        }

        // Push the new application layer
        let new_layer_digest = format!("sha256:{}", sha256::digest(&new_layer_data));
        debug!("Pushing new application layer: {}", new_layer_digest);
        self.push_blob(repository, &new_layer_data, &new_layer_digest, auth)
            .await?;

        // Create manifest with all layers (base + new)
        let mut manifest_layers = Vec::new();
        for layer in &manifest.layers {
            manifest_layers.push(OciDescriptor {
                media_type: layer.media_type.clone(),
                digest: layer.digest.clone(),
                size: layer.size,
                urls: None,
                annotations: None,
            });
        }

        // Create and push manifest
        let oci_manifest = OciImageManifest {
            schema_version: 2,
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            config: Some(OciDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: config_digest,
                size: config_data.len() as i64,
                urls: None,
                annotations: None,
            }),
            layers: manifest_layers,
            annotations: None,
        };

        let (_, digest) = self.push_manifest(repository, &oci_manifest, auth).await?;
        let digest_ref = format!(
            "{}/{}@{}",
            target_reference.registry, target_reference.repository, digest
        );
        // Calculate the actual manifest size that was pushed
        let manifest_json = serde_json::to_vec_pretty(&oci_manifest)?;
        let manifest_size = manifest_json.len();

        info!(
            "Successfully pushed layered image to {} (digest: {})",
            digest_ref, digest
        );

        Ok((digest_ref, manifest_size))
    }

    pub async fn push_manifest_list(
        &mut self,
        image_ref: &str,
        manifest_descriptors: Vec<crate::manifest::ManifestDescriptor>,
        auth: &RegistryAuth,
        push_tag: bool,
    ) -> Result<String> {
        let reference = ImageReference::parse(image_ref)?;

        // Create the image index
        let index = crate::manifest::ImageIndex::new(manifest_descriptors);

        // Convert to OCI index
        let oci_manifests: Vec<ImageIndexEntry> = index
            .manifests
            .iter()
            .map(|m| ImageIndexEntry {
                media_type: m.media_type.clone(),
                digest: m.digest.clone(),
                size: m.size,
                platform: Some(Platform {
                    architecture: m.platform.architecture.clone(),
                    os: m.platform.os.clone(),
                    variant: m.platform.variant.clone(),
                }),
                annotations: None,
            })
            .collect();

        let oci_index = OciImageIndex {
            schema_version: 2,
            media_type: "application/vnd.oci.image.index.v1+json".to_string(),
            manifests: oci_manifests,
            annotations: None,
        };

        debug!(
            "Pushing manifest list with {} manifests",
            index.manifests.len()
        );
        for m in &index.manifests {
            debug!(
                "  - Platform: {}/{}, digest: {}",
                m.platform.os, m.platform.architecture, m.digest
            );
        }

        // Serialize and calculate digest
        let manifest_json = serde_json::to_vec_pretty(&oci_index)?;
        let manifest_digest = format!("sha256:{}", sha256::digest(&manifest_json));

        // Push by digest or tag based on push_tag flag
        let manifest_ref = if push_tag {
            reference.tag.as_deref().unwrap_or("latest")
        } else {
            &manifest_digest
        };

        let url = format!(
            "https://{}/v2/{}/manifests/{}",
            reference.registry, reference.repository, manifest_ref
        );

        let token = self
            .authenticate(&reference.registry, &reference.repository, auth)
            .await?;

        let mut req = self
            .client
            .put(&url)
            .header("Content-Type", "application/vnd.oci.image.index.v1+json")
            .body(manifest_json.clone());

        if let Some(token) = token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to push manifest list: {}", response.status());
        }

        // Get digest from response or use the calculated one
        let digest = response
            .headers()
            .get("docker-content-digest")
            .and_then(|h| h.to_str().ok())
            .unwrap_or(&manifest_digest)
            .to_string();

        // Always return by digest
        let image_ref = format!("{}/{}@{}", reference.registry, reference.repository, digest);

        Ok(image_ref)
    }
}

pub fn parse_image_reference(image: &str) -> Result<(String, String, String)> {
    let reference = ImageReference::parse(image)?;
    let tag = reference.tag.as_deref().unwrap_or("latest").to_string();
    Ok((reference.registry, reference.repository, tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    // Tests don't currently use these imports but kept for future use

    #[test]
    fn test_parse_image_reference() {
        let (registry, repo, tag) =
            parse_image_reference("docker.io/library/hello-world:latest").unwrap();
        assert_eq!(registry, "registry-1.docker.io");
        assert_eq!(repo, "library/hello-world");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_parse_image_reference_no_tag() {
        let (_, _, tag) = parse_image_reference("docker.io/library/hello-world").unwrap();
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_image_reference_parsing() {
        let ref1 = ImageReference::parse("alpine:latest").unwrap();
        assert_eq!(ref1.registry, "registry-1.docker.io");
        assert_eq!(ref1.repository, "library/alpine");
        assert_eq!(ref1.tag, Some("latest".to_string()));

        let ref2 = ImageReference::parse("cgr.dev/chainguard/static:latest").unwrap();
        assert_eq!(ref2.registry, "cgr.dev");
        assert_eq!(ref2.repository, "chainguard/static");
        assert_eq!(ref2.tag, Some("latest".to_string()));

        let ref3 = ImageReference::parse("ttl.sh/test/app@sha256:abc123").unwrap();
        assert_eq!(ref3.registry, "ttl.sh");
        assert_eq!(ref3.repository, "test/app");
        assert_eq!(ref3.digest, Some("sha256:abc123".to_string()));
    }

    #[test]
    fn test_image_reference_parsing_docker_hub() {
        // Test basic Docker Hub image (no registry specified)
        let ref1 = ImageReference::parse("ubuntu").unwrap();
        assert_eq!(ref1.registry, "registry-1.docker.io");
        assert_eq!(ref1.repository, "library/ubuntu");
        assert_eq!(ref1.tag, None);
        assert_eq!(ref1.digest, None);

        // Test Docker Hub image with tag
        let ref2 = ImageReference::parse("ubuntu:20.04").unwrap();
        assert_eq!(ref2.registry, "registry-1.docker.io");
        assert_eq!(ref2.repository, "library/ubuntu");
        assert_eq!(ref2.tag, Some("20.04".to_string()));
        assert_eq!(ref2.digest, None);

        // Test Docker Hub user image
        let ref3 = ImageReference::parse("nginx/nginx:latest").unwrap();
        assert_eq!(ref3.registry, "registry-1.docker.io");
        assert_eq!(ref3.repository, "nginx/nginx");
        assert_eq!(ref3.tag, Some("latest".to_string()));

        // Test explicit docker.io (should redirect to registry-1.docker.io)
        let ref4 = ImageReference::parse("docker.io/library/alpine:3.18").unwrap();
        assert_eq!(ref4.registry, "registry-1.docker.io");
        assert_eq!(ref4.repository, "library/alpine");
        assert_eq!(ref4.tag, Some("3.18".to_string()));

        // Test explicit docker.io with user repo
        let ref5 = ImageReference::parse("docker.io/user/repo:tag").unwrap();
        assert_eq!(ref5.registry, "registry-1.docker.io");
        assert_eq!(ref5.repository, "user/repo");
        assert_eq!(ref5.tag, Some("tag".to_string()));
    }

    #[test]
    fn test_image_reference_parsing_digests() {
        // Test image with digest only
        let ref1 = ImageReference::parse("alpine@sha256:1234567890abcdef").unwrap();
        assert_eq!(ref1.registry, "registry-1.docker.io");
        assert_eq!(ref1.repository, "library/alpine");
        assert_eq!(ref1.tag, None);
        assert_eq!(ref1.digest, Some("sha256:1234567890abcdef".to_string()));

        // Test registry with digest
        let ref2 = ImageReference::parse("gcr.io/project/image@sha256:abcdef1234567890").unwrap();
        assert_eq!(ref2.registry, "gcr.io");
        assert_eq!(ref2.repository, "project/image");
        assert_eq!(ref2.tag, None);
        assert_eq!(ref2.digest, Some("sha256:abcdef1234567890".to_string()));

        // Test long digest
        let ref3 = ImageReference::parse("quay.io/user/repo@sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap();
        assert_eq!(ref3.registry, "quay.io");
        assert_eq!(ref3.repository, "user/repo");
        assert_eq!(
            ref3.digest,
            Some(
                "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_image_reference_parsing_registries() {
        // Test Google Container Registry
        let ref1 = ImageReference::parse("gcr.io/my-project/my-app:v1.0").unwrap();
        assert_eq!(ref1.registry, "gcr.io");
        assert_eq!(ref1.repository, "my-project/my-app");
        assert_eq!(ref1.tag, Some("v1.0".to_string()));

        // Test Google Artifact Registry
        let ref2 =
            ImageReference::parse("us-central1-docker.pkg.dev/project/repo/image:latest").unwrap();
        assert_eq!(ref2.registry, "us-central1-docker.pkg.dev");
        assert_eq!(ref2.repository, "project/repo/image");
        assert_eq!(ref2.tag, Some("latest".to_string()));

        // Test Quay.io
        let ref3 = ImageReference::parse("quay.io/organization/repository:tag").unwrap();
        assert_eq!(ref3.registry, "quay.io");
        assert_eq!(ref3.repository, "organization/repository");
        assert_eq!(ref3.tag, Some("tag".to_string()));

        // Test GitHub Container Registry
        let ref4 = ImageReference::parse("ghcr.io/user/repo:main").unwrap();
        assert_eq!(ref4.registry, "ghcr.io");
        assert_eq!(ref4.repository, "user/repo");
        assert_eq!(ref4.tag, Some("main".to_string()));

        // Test Amazon ECR
        let ref5 =
            ImageReference::parse("123456789012.dkr.ecr.us-west-2.amazonaws.com/my-repo:latest")
                .unwrap();
        assert_eq!(
            ref5.registry,
            "123456789012.dkr.ecr.us-west-2.amazonaws.com"
        );
        assert_eq!(ref5.repository, "my-repo");
        assert_eq!(ref5.tag, Some("latest".to_string()));

        // Test ttl.sh (ephemeral registry)
        let ref6 = ImageReference::parse("ttl.sh/user/image:1h").unwrap();
        assert_eq!(ref6.registry, "ttl.sh");
        assert_eq!(ref6.repository, "user/image");
        assert_eq!(ref6.tag, Some("1h".to_string()));
    }

    #[test]
    fn test_image_reference_parsing_localhost() {
        // Test localhost registry
        let ref1 = ImageReference::parse("localhost:5000/my-image:latest").unwrap();
        assert_eq!(ref1.registry, "localhost:5000");
        assert_eq!(ref1.repository, "my-image");
        assert_eq!(ref1.tag, Some("latest".to_string()));

        // Test localhost without port
        let ref2 = ImageReference::parse("localhost/test:v1").unwrap();
        assert_eq!(ref2.registry, "localhost");
        assert_eq!(ref2.repository, "test");
        assert_eq!(ref2.tag, Some("v1".to_string()));

        // Test IP address registry
        let ref3 = ImageReference::parse("192.168.1.100:8080/app:dev").unwrap();
        assert_eq!(ref3.registry, "192.168.1.100:8080");
        assert_eq!(ref3.repository, "app");
        assert_eq!(ref3.tag, Some("dev".to_string()));
    }

    #[test]
    fn test_image_reference_parsing_edge_cases() {
        // Test image with no tag (should default to None)
        let ref1 = ImageReference::parse("alpine").unwrap();
        assert_eq!(ref1.registry, "registry-1.docker.io");
        assert_eq!(ref1.repository, "library/alpine");
        assert_eq!(ref1.tag, None);
        assert_eq!(ref1.digest, None);

        // Test deep repository path
        let ref2 = ImageReference::parse("gcr.io/project/team/service/component:v2.1.0").unwrap();
        assert_eq!(ref2.registry, "gcr.io");
        assert_eq!(ref2.repository, "project/team/service/component");
        assert_eq!(ref2.tag, Some("v2.1.0".to_string()));

        // Test tag that looks like a port number
        let ref3 = ImageReference::parse("myregistry.com:443/repo:5000").unwrap();
        assert_eq!(ref3.registry, "myregistry.com:443");
        assert_eq!(ref3.repository, "repo");
        assert_eq!(ref3.tag, Some("5000".to_string()));

        // Test complex tag with special characters
        let ref4 = ImageReference::parse("example.com/app:v1.2.3-alpha.1").unwrap();
        assert_eq!(ref4.registry, "example.com");
        assert_eq!(ref4.repository, "app");
        assert_eq!(ref4.tag, Some("v1.2.3-alpha.1".to_string()));

        // Test underscore in repository name
        let ref5 = ImageReference::parse("docker.io/my_user/my_repo:latest").unwrap();
        assert_eq!(ref5.registry, "registry-1.docker.io");
        assert_eq!(ref5.repository, "my_user/my_repo");
        assert_eq!(ref5.tag, Some("latest".to_string()));
    }

    #[test]
    fn test_image_reference_whitespace_handling() {
        // Test with leading/trailing whitespace
        let ref1 = ImageReference::parse("  alpine:latest  ").unwrap();
        assert_eq!(ref1.registry, "registry-1.docker.io");
        assert_eq!(ref1.repository, "library/alpine");
        assert_eq!(ref1.tag, Some("latest".to_string()));

        // Test with tabs
        let ref2 = ImageReference::parse("\tgcr.io/project/app:v1\t").unwrap();
        assert_eq!(ref2.registry, "gcr.io");
        assert_eq!(ref2.repository, "project/app");
        assert_eq!(ref2.tag, Some("v1".to_string()));
    }

    #[test]
    fn test_image_reference_reference_method() {
        // Test reference() method with tag
        let ref1 = ImageReference::parse("alpine:3.18").unwrap();
        assert_eq!(ref1.reference(), "registry-1.docker.io/library/alpine:3.18");

        // Test reference() method with digest
        let ref2 = ImageReference::parse("alpine@sha256:abc123").unwrap();
        assert_eq!(
            ref2.reference(),
            "registry-1.docker.io/library/alpine@sha256:abc123"
        );

        // Test reference() method with no tag (should default to latest)
        let ref3 = ImageReference::parse("alpine").unwrap();
        assert_eq!(
            ref3.reference(),
            "registry-1.docker.io/library/alpine:latest"
        );

        // Test reference() method with registry
        let ref4 = ImageReference::parse("gcr.io/project/app:v1").unwrap();
        assert_eq!(ref4.reference(), "gcr.io/project/app:v1");
    }

    #[test]
    fn test_image_reference_repository_url_method() {
        // Test repository_url() method
        let ref1 = ImageReference::parse("alpine:latest").unwrap();
        assert_eq!(ref1.repository_url(), "registry-1.docker.io/library/alpine");

        let ref2 = ImageReference::parse("gcr.io/my-project/my-app:v1").unwrap();
        assert_eq!(ref2.repository_url(), "gcr.io/my-project/my-app");

        let ref3 = ImageReference::parse("localhost:5000/test@sha256:abc").unwrap();
        assert_eq!(ref3.repository_url(), "localhost:5000/test");
    }
}
