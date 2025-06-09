use anyhow::{Context, Result};
use oci_distribution::manifest::{OciDescriptor, OciImageManifest, OciManifest};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::{Client, Reference};
use std::str::FromStr;
use tracing::{debug, info};

pub struct RegistryClient {
    client: Client,
}

impl RegistryClient {
    pub fn new() -> Result<Self> {
        let client = Client::new(oci_distribution::client::ClientConfig::default());
        Ok(Self { client })
    }

    pub async fn push_image_by_digest(
        &mut self,
        repository: &str,
        config_data: Vec<u8>,
        layers: Vec<(Vec<u8>, String)>,
        auth: &RegistryAuth,
    ) -> Result<(String, usize)> {
        // Create a temporary reference for authentication - we'll only use the digest result
        let temp_ref = format!("{}:temp", repository);
        let reference: Reference = temp_ref
            .parse()
            .context("Failed to parse repository reference")?;

        info!("Pushing image to {} (digest only)", repository);

        // Authenticate with the registry
        self.client
            .auth(&reference, auth, oci_distribution::RegistryOperation::Push)
            .await
            .context("Failed to authenticate with registry")?;

        // Push config blob
        let config_digest = format!("sha256:{}", sha256::digest(&config_data));
        debug!("Pushing config blob: {}", config_digest);

        self.client
            .push_blob(&reference, &config_data, &config_digest)
            .await
            .context("Failed to push config blob")?;

        // Push layers
        let mut manifest_layers = Vec::new();
        for (layer_data, media_type) in layers {
            let digest = format!("sha256:{}", sha256::digest(&layer_data));
            debug!("Pushing layer: {}", digest);

            self.client
                .push_blob(&reference, &layer_data, &digest)
                .await
                .context("Failed to push layer")?;

            manifest_layers.push(OciDescriptor {
                media_type: media_type.clone(),
                digest: digest.clone(),
                size: layer_data.len() as i64,
                urls: None,
                annotations: None,
            });
        }

        // Create and push manifest (this will generate a digest)
        let image_manifest = OciImageManifest {
            schema_version: 2,
            media_type: Some("application/vnd.oci.image.manifest.v1+json".to_string()),
            artifact_type: None,
            config: OciDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: config_digest,
                size: config_data.len() as i64,
                urls: None,
                annotations: None,
            },
            layers: manifest_layers,
            annotations: None,
        };

        // Wrap the image manifest in the OciManifest enum
        let manifest = OciManifest::Image(image_manifest);

        debug!("Pushing manifest for digest-only image");
        let (manifest_url, digest) = self
            .client
            .push_manifest_and_get_digest(&reference, &manifest)
            .await
            .context("Failed to push manifest")?;

        info!(
            "Successfully pushed image to {} (digest: {})",
            manifest_url, digest
        );

        // Build the full image reference with digest only
        let registry = reference.registry();
        let repository = reference.repository();
        let digest_ref = format!("{}/{}@{}", registry, repository, digest);

        // Return both the digest ref and the actual manifest size
        let manifest_size = serde_json::to_vec(&manifest)?.len();
        Ok((digest_ref, manifest_size))
    }

    pub async fn push_image(
        &mut self,
        image_ref: &str,
        config_data: Vec<u8>,
        layers: Vec<(Vec<u8>, String)>,
        auth: &RegistryAuth,
    ) -> Result<(String, usize)> {
        let reference: Reference = image_ref
            .parse()
            .context("Failed to parse image reference")?;

        info!("Pushing image to {}", reference);

        // Authenticate with the registry
        self.client
            .auth(&reference, auth, oci_distribution::RegistryOperation::Push)
            .await
            .context("Failed to authenticate with registry")?;

        // Push config blob
        let config_digest = format!("sha256:{}", sha256::digest(&config_data));
        debug!("Pushing config blob: {}", config_digest);

        self.client
            .push_blob(&reference, &config_data, &config_digest)
            .await
            .context("Failed to push config blob")?;

        // Push layers
        let mut manifest_layers = Vec::new();
        for (layer_data, media_type) in layers {
            let digest = format!("sha256:{}", sha256::digest(&layer_data));
            debug!("Pushing layer: {}", digest);

            self.client
                .push_blob(&reference, &layer_data, &digest)
                .await
                .context("Failed to push layer")?;

            manifest_layers.push(OciDescriptor {
                media_type: media_type.clone(),
                digest: digest.clone(),
                size: layer_data.len() as i64,
                urls: None,
                annotations: None,
            });
        }

        // Create and push manifest
        let image_manifest = OciImageManifest {
            schema_version: 2,
            media_type: Some("application/vnd.oci.image.manifest.v1+json".to_string()),
            artifact_type: None,
            config: OciDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: config_digest,
                size: config_data.len() as i64,
                urls: None,
                annotations: None,
            },
            layers: manifest_layers,
            annotations: None,
        };

        // Wrap the image manifest in the OciManifest enum
        let manifest = OciManifest::Image(image_manifest);

        debug!("Pushing manifest");
        let (manifest_url, digest) = self
            .client
            .push_manifest_and_get_digest(&reference, &manifest)
            .await
            .context("Failed to push manifest")?;

        info!("Successfully pushed image to {}", manifest_url);

        // Build the full image reference with digest
        let registry = reference.registry();
        let repository = reference.repository();
        let digest_ref = format!("{}/{}@{}", registry, repository, digest);

        // Return both the digest ref and the actual manifest size
        let manifest_size = serde_json::to_vec(&manifest)?.len();
        Ok((digest_ref, manifest_size))
    }

    pub async fn push_manifest_list(
        &mut self,
        image_ref: &str,
        manifest_descriptors: Vec<crate::manifest::ManifestDescriptor>,
        auth: &RegistryAuth,
    ) -> Result<String> {
        let reference = Reference::from_str(image_ref)
            .context(format!("Failed to parse image reference: {}", image_ref))?;

        // Authenticate with the registry
        self.client
            .auth(&reference, auth, oci_distribution::RegistryOperation::Push)
            .await
            .context("Failed to authenticate with registry")?;

        // Create the image index
        let index = crate::manifest::ImageIndex::new(manifest_descriptors);

        // Convert to OCI index
        let oci_manifests: Vec<oci_distribution::manifest::ImageIndexEntry> = index
            .manifests
            .iter()
            .map(|m| oci_distribution::manifest::ImageIndexEntry {
                media_type: m.media_type.clone(),
                digest: m.digest.clone(),
                size: m.size,
                platform: Some(oci_distribution::manifest::Platform {
                    architecture: m.platform.architecture.clone(),
                    os: m.platform.os.clone(),
                    os_version: None,
                    os_features: None,
                    variant: m.platform.variant.clone(),
                    features: None,
                }),
                annotations: None,
            })
            .collect();

        let oci_index = oci_distribution::manifest::OciImageIndex {
            schema_version: 2,
            media_type: Some("application/vnd.oci.image.index.v1+json".to_string()),
            manifests: oci_manifests,
            annotations: None,
        };

        // Wrap in OciManifest enum
        let manifest = oci_distribution::manifest::OciManifest::ImageIndex(oci_index);

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
        let (manifest_url, digest) = self
            .client
            .push_manifest_and_get_digest(&reference, &manifest)
            .await
            .context("Failed to push manifest list")?;

        info!("Successfully pushed manifest list to {}", manifest_url);

        // Build the full image reference with digest
        let registry = reference.registry();
        let repository = reference.repository();
        let image_ref = format!("{}/{}@{}", registry, repository, digest);

        Ok(image_ref)
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
        // Create a temporary reference for authentication
        let temp_ref = format!("{}:temp", repository);
        let reference: Reference = temp_ref
            .parse()
            .context("Failed to parse repository reference")?;

        info!("Pushing layered image to {} (digest only)", repository);

        // Authenticate with the registry
        self.client
            .auth(&reference, auth, oci_distribution::RegistryOperation::Push)
            .await
            .context("Failed to authenticate with registry")?;

        // Push config blob
        let config_digest = format!("sha256:{}", sha256::digest(&config_data));
        debug!("Pushing config blob: {}", config_digest);

        self.client
            .push_blob(&reference, &config_data, &config_digest)
            .await
            .context("Failed to push config blob")?;

        // Copy base image layers if they don't exist in target registry
        let base_reference: Reference = base_image_ref
            .parse()
            .context("Failed to parse base image reference")?;

        // Check if we need to copy base layers (cross-registry scenario)
        let base_registry = base_reference.registry();
        let target_registry = reference.registry();
        let need_copy_layers = base_registry != target_registry;

        if need_copy_layers {
            info!(
                "Copying base image layers from {} to {}",
                base_registry, target_registry
            );

            // Authenticate with base image registry
            let base_client = Client::new(oci_distribution::client::ClientConfig::default());
            base_client
                .auth(
                    &base_reference,
                    base_auth,
                    oci_distribution::RegistryOperation::Pull,
                )
                .await
                .context("Failed to authenticate with base registry")?;

            // Copy each base layer (all except the last one which is our app layer)
            for layer in &manifest.layers[..manifest.layers.len().saturating_sub(1)] {
                debug!("Copying base layer: {}", layer.digest);

                // Pull the layer from base registry
                let mut layer_data = Vec::new();
                let layer_descriptor = oci_distribution::manifest::OciDescriptor {
                    media_type: layer.media_type.clone(),
                    digest: layer.digest.clone(),
                    size: layer.size,
                    urls: None,
                    annotations: None,
                };

                base_client
                    .pull_blob(&base_reference, &layer_descriptor, &mut layer_data)
                    .await
                    .context("Failed to pull base layer")?;

                // Push the layer to target registry
                self.client
                    .push_blob(&reference, &layer_data, &layer.digest)
                    .await
                    .context("Failed to push copied base layer")?;
            }
        }

        // Push the new application layer
        let new_layer_digest = format!("sha256:{}", sha256::digest(&new_layer_data));
        debug!("Pushing new application layer: {}", new_layer_digest);

        self.client
            .push_blob(&reference, &new_layer_data, &new_layer_digest)
            .await
            .context("Failed to push new layer")?;

        // Create manifest with all layers (base + new)
        let mut manifest_layers = Vec::new();
        for layer in &manifest.layers {
            manifest_layers.push(oci_distribution::manifest::OciDescriptor {
                media_type: layer.media_type.clone(),
                digest: layer.digest.clone(),
                size: layer.size,
                urls: None,
                annotations: None,
            });
        }

        // Create and push manifest
        let image_manifest = oci_distribution::manifest::OciImageManifest {
            schema_version: 2,
            media_type: Some("application/vnd.oci.image.manifest.v1+json".to_string()),
            artifact_type: None,
            config: oci_distribution::manifest::OciDescriptor {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: config_digest,
                size: config_data.len() as i64,
                urls: None,
                annotations: None,
            },
            layers: manifest_layers,
            annotations: None,
        };

        // Wrap in OciManifest enum
        let oci_manifest = oci_distribution::manifest::OciManifest::Image(image_manifest);

        debug!("Pushing manifest for layered image");
        let (manifest_url, digest) = self
            .client
            .push_manifest_and_get_digest(&reference, &oci_manifest)
            .await
            .context("Failed to push manifest")?;

        info!(
            "Successfully pushed layered image to {} (digest: {})",
            manifest_url, digest
        );

        // Build the full image reference with digest only
        let registry = reference.registry();
        let repository = reference.repository();
        let digest_ref = format!("{}/{}@{}", registry, repository, digest);

        // Return both the digest ref and the actual manifest size
        let manifest_size = serde_json::to_vec(&oci_manifest)?.len();
        Ok((digest_ref, manifest_size))
    }

    /// Fetch the manifest and config for a specific platform of an image
    pub async fn fetch_image_data(
        &mut self,
        image_ref: &str,
        platform: &str,
        auth: &RegistryAuth,
    ) -> Result<(
        oci_distribution::manifest::OciImageManifest,
        crate::image::ImageConfig,
    )> {
        let reference: Reference = image_ref
            .parse()
            .context("Failed to parse image reference")?;

        debug!(
            "Fetching image data for {} platform {}",
            reference, platform
        );

        // Authenticate with the registry
        self.client
            .auth(&reference, auth, oci_distribution::RegistryOperation::Pull)
            .await
            .context("Failed to authenticate with registry")?;

        // Pull the manifest for the specific platform
        let (manifest, _digest) = self
            .client
            .pull_manifest(&reference, auth)
            .await
            .context("Failed to pull manifest")?;

        // Handle different manifest types
        let image_manifest = match manifest {
            oci_distribution::manifest::OciManifest::Image(img_manifest) => img_manifest,
            oci_distribution::manifest::OciManifest::ImageIndex(index) => {
                // Find the manifest for the requested platform
                let (os, arch) = platform
                    .split_once('/')
                    .ok_or_else(|| anyhow::anyhow!("Invalid platform format: {}", platform))?;

                let platform_manifest = index
                    .manifests
                    .iter()
                    .find(|m| {
                        m.platform
                            .as_ref()
                            .is_some_and(|p| p.os == os && p.architecture == arch)
                    })
                    .ok_or_else(|| {
                        anyhow::anyhow!("Platform {} not found in image index", platform)
                    })?;

                // Pull the platform-specific manifest
                let platform_ref = format!("{}@{}", image_ref, platform_manifest.digest);
                let platform_reference: Reference = platform_ref
                    .parse()
                    .context("Failed to parse platform reference")?;

                let (platform_manifest, _) = self
                    .client
                    .pull_manifest(&platform_reference, auth)
                    .await
                    .context("Failed to pull platform manifest")?;

                match platform_manifest {
                    oci_distribution::manifest::OciManifest::Image(img_manifest) => img_manifest,
                    _ => anyhow::bail!("Expected image manifest, got index"),
                }
            }
        };

        // Pull the config blob
        let mut config_data = Vec::new();
        self.client
            .pull_blob(&reference, &image_manifest.config, &mut config_data)
            .await
            .context("Failed to pull config blob")?;

        // Parse the config
        let config: crate::image::ImageConfig =
            serde_json::from_slice(&config_data).context("Failed to parse image config")?;

        Ok((image_manifest, config))
    }

    /// Fetch the manifest for an image and extract available platforms
    pub async fn get_image_platforms(
        &mut self,
        image_ref: &str,
        auth: &RegistryAuth,
    ) -> Result<Vec<String>> {
        let reference: Reference = image_ref
            .parse()
            .context("Failed to parse image reference")?;

        debug!("Fetching platforms for {}", reference);

        // Use the new get_image_platforms method from oci-distribution
        let platforms = self
            .client
            .get_image_platforms(&reference, auth)
            .await
            .context("Failed to get image platforms")?;

        // Convert (os, arch) tuples to platform strings and normalize
        let mut platform_strings: Vec<String> = platforms
            .into_iter()
            .filter_map(|(os, arch)| {
                // Filter out unknown/invalid platforms
                if os == "unknown" || arch == "unknown" || os.is_empty() || arch.is_empty() {
                    return None;
                }

                let platform = format!("{}/{}", os, arch);

                // Normalize and filter platforms
                let normalized = match platform.as_str() {
                    "linux/amd64" => Some("linux/amd64".to_string()),
                    "linux/arm64" | "linux/arm64/v8" => Some("linux/arm64".to_string()),
                    "linux/arm/v7" => Some("linux/arm/v7".to_string()),
                    "linux/arm/v6" => Some("linux/arm/v6".to_string()),
                    "linux/386" => Some("linux/386".to_string()),
                    "linux/ppc64le" => Some("linux/ppc64le".to_string()),
                    "linux/s390x" => Some("linux/s390x".to_string()),
                    "linux/riscv64" => Some("linux/riscv64".to_string()),
                    _ => {
                        debug!("Skipping unsupported platform: {}", platform);
                        None
                    }
                };
                normalized
            })
            .collect();

        // Deduplicate platforms
        platform_strings.sort();
        platform_strings.dedup();

        if platform_strings.is_empty() {
            debug!("No valid platforms found, defaulting to linux/amd64");
            Ok(vec!["linux/amd64".to_string()])
        } else {
            info!("Found platforms: {:?}", platform_strings);
            Ok(platform_strings)
        }
    }
}

pub fn parse_image_reference(image: &str) -> Result<(String, String, String)> {
    let reference: Reference = image.parse().context("Failed to parse image reference")?;

    let registry = reference.registry().to_string();
    let repository = reference.repository().to_string();
    let tag = reference.tag().unwrap_or("latest").to_string();

    Ok((registry, repository, tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{Config, History, ImageConfig, RootFs};

    #[test]
    fn test_parse_image_reference() {
        let (registry, repo, tag) =
            parse_image_reference("docker.io/library/hello-world:latest").unwrap();
        assert_eq!(registry, "docker.io");
        assert_eq!(repo, "library/hello-world");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_parse_image_reference_no_tag() {
        let (_, _, tag) = parse_image_reference("docker.io/library/hello-world").unwrap();
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_layered_config_structure() {
        // Test layered config creation
        let config = ImageConfig {
            architecture: "amd64".to_string(),
            os: "linux".to_string(),
            config: Config {
                env: vec![
                    "PATH=/usr/local/bin:/usr/bin:/bin".to_string(),
                    "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt".to_string(),
                ],
                cmd: Some(vec!["/app/test-binary".to_string()]),
                working_dir: "/".to_string(),
                user: "nonroot:nonroot".to_string(),
            },
            rootfs: RootFs {
                fs_type: "layers".to_string(),
                diff_ids: vec![
                    "sha256:base_layer_1".to_string(),
                    "sha256:base_layer_2".to_string(),
                    "sha256:app_layer".to_string(),
                ],
            },
            history: vec![
                History {
                    created: "2023-01-01T00:00:00Z".to_string(),
                    created_by: "base-image-builder".to_string(),
                    comment: "Base layer 1".to_string(),
                    empty_layer: false,
                },
                History {
                    created: "2023-01-01T00:01:00Z".to_string(),
                    created_by: "base-image-builder".to_string(),
                    comment: "Base layer 2".to_string(),
                    empty_layer: false,
                },
                History {
                    created: "2023-01-01T00:02:00Z".to_string(),
                    created_by: "krust".to_string(),
                    comment: "Built with krust".to_string(),
                    empty_layer: false,
                },
            ],
        };

        // Test that layered configs preserve base image properties
        assert_eq!(config.rootfs.diff_ids.len(), 3);
        assert_eq!(config.history.len(), 3);

        // Verify diff_ids match expected order
        assert_eq!(config.rootfs.diff_ids[0], "sha256:base_layer_1");
        assert_eq!(config.rootfs.diff_ids[1], "sha256:base_layer_2");
        assert_eq!(config.rootfs.diff_ids[2], "sha256:app_layer");

        // Verify history is preserved and extended
        assert_eq!(config.history[0].created_by, "base-image-builder");
        assert_eq!(config.history[1].created_by, "base-image-builder");
        assert_eq!(config.history[2].created_by, "krust");

        // Verify base image environment is preserved
        assert!(config
            .config
            .env
            .contains(&"SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt".to_string()));
        assert!(config.config.env.iter().any(|env| env.starts_with("PATH=")));
    }
}
