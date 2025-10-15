//! Platform detection service
//!
//! Handles detection of available platforms from base images.

use anyhow::Result;
use crate::registry::{RegistryAuth, RegistryClient};
use tracing::info;

/// Service for detecting available platforms
pub struct PlatformDetector;

impl PlatformDetector {
    /// Detect platforms from a base image, or return defaults
    pub async fn detect_platforms(
        base_image: &str,
        registry_client: &mut RegistryClient,
        auth: &RegistryAuth,
    ) -> Result<Vec<String>> {
        info!(
            "Detecting available platforms from base image: {}",
            base_image
        );

        match registry_client
            .get_image_platforms(base_image, auth)
            .await
        {
            Ok(detected_platforms) => {
                if detected_platforms.is_empty() {
                    info!("No platforms detected, using defaults");
                    Ok(Self::default_platforms())
                } else {
                    info!("Detected platforms: {:?}", detected_platforms);
                    Ok(detected_platforms)
                }
            }
            Err(e) => {
                info!("Failed to detect platforms: {}. Using defaults.", e);
                Ok(Self::default_platforms())
            }
        }
    }

    /// Get default platforms when detection fails or returns empty
    fn default_platforms() -> Vec<String> {
        vec!["linux/amd64".to_string(), "linux/arm64".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_platforms() {
        let platforms = PlatformDetector::default_platforms();
        assert_eq!(platforms.len(), 2);
        assert_eq!(platforms[0], "linux/amd64");
        assert_eq!(platforms[1], "linux/arm64");
    }
}
