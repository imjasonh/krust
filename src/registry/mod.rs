use anyhow::{Context, Result};
use oci_distribution::manifest::{OciDescriptor, OciImageManifest, OciManifest};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::{Client, Reference};
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
    ) -> Result<String> {
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
            .last()
            .context("Failed to extract digest from manifest URL")?;
        
        // Build the full image reference with digest
        let registry = reference.registry();
        let repository = reference.repository();
        let digest_ref = format!("{}/{}@{}", registry, repository, digest);
        
        Ok(digest_ref)
    }
}

pub fn parse_image_reference(image: &str) -> Result<(String, String, String)> {
    let reference: Reference = image.parse().context("Failed to parse image reference")?;

    let registry = reference.registry().to_string();
    let repository = reference.repository().to_string();
    let tag = reference.tag().unwrap_or("latest").to_string();

    Ok((registry, repository, tag))
}
