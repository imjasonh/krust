//! Build service for orchestrating the build process
//!
//! Handles building Rust binaries, creating container images, and pushing to registries.

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

use crate::{
    auth::resolve_auth,
    builder::{get_rust_target_triple, RustBuilder},
    image::ImageBuilder,
    manifest::{ManifestDescriptor, Platform},
    registry::RegistryClient,
};

/// Configuration for a build operation
pub struct BuildConfig {
    pub project_path: PathBuf,
    pub base_image: String,
    pub target_repo: String,
    pub platforms: Vec<String>,
    pub no_push: bool,
    pub tag: Option<String>,
    pub cargo_args: Vec<String>,
}

/// Result of a build operation
pub struct BuildResult {
    pub image_ref: Option<String>,
}

/// Service for orchestrating the build process
pub struct BuildService;

impl BuildService {
    /// Build and optionally push a container image for the given configuration
    pub async fn build(config: BuildConfig) -> Result<BuildResult> {
        let mut registry_client = RegistryClient::new()?;
        let mut manifest_descriptors = Vec::new();

        // Build for each platform
        for platform_str in &config.platforms {
            info!("Building for platform: {}", platform_str);

            // Build the Rust binary for this platform
            let target = get_rust_target_triple(platform_str)?;
            let builder = RustBuilder::new(&config.project_path, &target)
                .with_cargo_args(config.cargo_args.clone());

            let build_result = builder.build()?;

            // Build container image for this platform
            let image_builder = ImageBuilder::new(
                build_result.binary_path,
                config.base_image.clone(),
                platform_str.clone(),
            );

            // Fetch base image and build image
            let base_auth = resolve_auth(&config.base_image)?;
            let (config_data, layer_data, manifest) = image_builder
                .build(&mut registry_client, &base_auth)
                .await?;

            // Push platform-specific image if not --no-push
            if !config.no_push {
                let descriptor = Self::push_platform_image(
                    &mut registry_client,
                    &config.target_repo,
                    &config.base_image,
                    platform_str,
                    config_data,
                    layer_data,
                    &manifest,
                )
                .await?;

                manifest_descriptors.push(descriptor);
            }
        }

        // Push manifest list if not --no-push
        let image_ref = if !config.no_push {
            Some(
                Self::push_manifest_list(
                    &mut registry_client,
                    &config.target_repo,
                    config.tag,
                    manifest_descriptors,
                )
                .await?,
            )
        } else {
            info!(
                "Successfully built image for {} platform(s)",
                config.platforms.len()
            );
            info!("Skipping push (--no-push specified)");
            None
        };

        Ok(BuildResult { image_ref })
    }

    /// Push a platform-specific image and return its manifest descriptor
    async fn push_platform_image(
        registry_client: &mut RegistryClient,
        target_repo: &str,
        base_image: &str,
        platform_str: &str,
        config_data: Vec<u8>,
        layer_data: Vec<u8>,
        manifest: &crate::image::Manifest,
    ) -> Result<ManifestDescriptor> {
        info!("Pushing image for platform: {}", platform_str);

        // Get auth for the target registry
        let push_auth = resolve_auth(target_repo)?;

        // Get the media type of the application layer (last layer in manifest)
        let app_layer_media_type = manifest
            .layers
            .last()
            .map(|l| l.media_type.clone())
            .unwrap_or_else(|| "application/vnd.docker.image.rootfs.diff.tar.gzip".to_string());

        // Get auth for base image
        let base_auth = resolve_auth(base_image)?;

        // Push layered image (copy base layers if needed + push app layer + manifest)
        let (digest_ref, manifest_size) = registry_client
            .push_layered_image(
                target_repo,
                config_data,
                layer_data,
                app_layer_media_type,
                manifest,
                &push_auth,
                base_image,
                &base_auth,
            )
            .await?;

        // Parse platform string
        let parts: Vec<&str> = platform_str.split('/').collect();
        let (os, arch) = if parts.len() >= 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            return Err(anyhow::anyhow!("Invalid platform format: {}", platform_str));
        };

        // Extract just the digest from the full reference
        let digest = digest_ref
            .split('@')
            .next_back()
            .unwrap_or("")
            .to_string();

        info!("Pushed platform image to: {}", digest_ref);

        // Add to manifest list
        info!(
            "Adding manifest to list - platform: {}/{}, digest: {}, size: {}",
            os, arch, digest, manifest_size
        );

        Ok(ManifestDescriptor {
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            size: manifest_size as i64,
            digest,
            platform: Platform {
                architecture: arch,
                os,
                variant: None,
            },
        })
    }

    /// Push manifest list and return the image reference
    async fn push_manifest_list(
        registry_client: &mut RegistryClient,
        target_repo: &str,
        tag: Option<String>,
        manifest_descriptors: Vec<ManifestDescriptor>,
    ) -> Result<String> {
        info!("Creating and pushing manifest list...");

        // Determine the target for the manifest list
        let manifest_target = if let Some(tag_name) = tag {
            // If --tag is specified, push to that tag
            format!("{}:{}", target_repo, tag_name)
        } else {
            // If no tag specified, push digest-only by using a temporary tag
            // We'll use a temporary tag and return the digest reference
            format!("{}:temp-{}", target_repo, std::process::id())
        };

        // Get auth for the final image push
        let final_auth = resolve_auth(&manifest_target)?;

        let manifest_list_ref = registry_client
            .push_manifest_list(&manifest_target, manifest_descriptors, &final_auth)
            .await?;

        Ok(manifest_list_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_config_creation() {
        let config = BuildConfig {
            project_path: PathBuf::from("/test"),
            base_image: "alpine:latest".to_string(),
            target_repo: "example.com/repo".to_string(),
            platforms: vec!["linux/amd64".to_string()],
            no_push: false,
            tag: Some("latest".to_string()),
            cargo_args: vec![],
        };

        assert_eq!(config.project_path, PathBuf::from("/test"));
        assert_eq!(config.base_image, "alpine:latest");
        assert_eq!(config.platforms.len(), 1);
    }
}
