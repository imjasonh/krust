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
    #[allow(dead_code)]
    auth: RegistryAuth,
}

impl RegistryClient {
    pub fn new(auth: RegistryAuth) -> Result<Self> {
        let client = Client::new(oci_distribution::client::ClientConfig::default());
        Ok(Self { client, auth })
    }

    pub async fn push_image(
        &mut self,
        image_ref: &str,
        config_data: Vec<u8>,
        layers: Vec<(Vec<u8>, String)>,
    ) -> Result<(String, usize)> {
        let reference: Reference = image_ref
            .parse()
            .context("Failed to parse image reference")?;

        info!("Pushing image to {}", reference);

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
        let manifest_url = self
            .client
            .push_manifest(&reference, &manifest)
            .await
            .context("Failed to push manifest")?;

        info!("Successfully pushed image to {}", manifest_url);

        // Extract digest from the manifest URL
        // URL format: https://registry/v2/repo/manifests/sha256:digest
        let digest = manifest_url
            .split('/')
            .next_back()
            .context("Failed to extract digest from manifest URL")?;

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
    ) -> Result<String> {
        let reference = Reference::from_str(image_ref)
            .context(format!("Failed to parse image reference: {}", image_ref))?;

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
        let manifest_url = self
            .client
            .push_manifest(&reference, &manifest)
            .await
            .context("Failed to push manifest list")?;

        info!("Successfully pushed manifest list to {}", manifest_url);

        // Extract digest from the manifest URL
        let digest = manifest_url
            .split('/')
            .next_back()
            .context("Failed to extract digest from manifest URL")?;

        // Build the full image reference with digest
        let registry = reference.registry();
        let repository = reference.repository();
        let image_ref = format!("{}/{}@{}", registry, repository, digest);

        Ok(image_ref)
    }

    /// Fetch the manifest for an image and extract available platforms
    pub async fn get_image_platforms(&mut self, image_ref: &str) -> Result<Vec<String>> {
        let reference: Reference = image_ref
            .parse()
            .context("Failed to parse image reference")?;

        debug!("Fetching manifest for {}", reference);

        // Pull the manifest
        let (manifest, _) = self
            .client
            .pull_manifest(&reference, &self.auth)
            .await
            .context("Failed to pull manifest")?;

        // Parse platforms based on manifest type
        match manifest {
            OciManifest::Image(_) => {
                // Single platform image - we need to fetch the config to determine platform
                debug!("Single platform image detected");
                // For now, we'll assume it's linux/amd64 if we can't determine
                // In a real implementation, we'd fetch the config blob
                Ok(vec!["linux/amd64".to_string()])
            }
            OciManifest::ImageIndex(index) => {
                // Multi-platform image - extract platforms from index
                debug!(
                    "Multi-platform image detected with {} manifests",
                    index.manifests.len()
                );
                let mut platforms: Vec<String> = index
                    .manifests
                    .iter()
                    .filter_map(|m| {
                        m.platform.as_ref().and_then(|p| {
                            // Filter out invalid platforms
                            if p.os == "unknown"
                                || p.architecture == "unknown"
                                || p.os.is_empty()
                                || p.architecture.is_empty()
                            {
                                return None;
                            }

                            let mut platform = format!("{}/{}", p.os, p.architecture);
                            if let Some(variant) = &p.variant {
                                if !variant.is_empty() {
                                    platform.push('/');
                                    platform.push_str(variant);
                                }
                            }
                            // Normalize and filter platforms
                            let normalized = match platform.as_str() {
                                "linux/amd64" => Some("linux/amd64".to_string()),
                                "linux/arm64" | "linux/arm64/v8" => Some("linux/arm64".to_string()),
                                "linux/arm/v6" | "linux/arm/v7" => Some("linux/arm/v7".to_string()),
                                _ => {
                                    debug!("Skipping unsupported platform: {}", platform);
                                    None
                                }
                            };
                            normalized
                        })
                    })
                    .collect();

                // Deduplicate platforms
                platforms.sort();
                platforms.dedup();

                info!("Found platforms: {:?}", platforms);
                Ok(platforms)
            }
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
