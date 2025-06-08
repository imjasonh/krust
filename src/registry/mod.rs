use anyhow::{Context, Result};
use oci_distribution::manifest::{OciDescriptor, OciImageManifest, OciManifest};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::{Client, Reference};
use std::str::FromStr;
use tracing::{debug, info};

#[cfg(test)]
mod tests;

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
