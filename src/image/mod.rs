use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use oci_distribution::secrets::RegistryAuth;
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::fs::File;
use std::io::Write;
use tar::Builder;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    pub architecture: String,
    pub os: String,
    pub config: Config,
    pub rootfs: RootFs,
    #[serde(default)]
    pub history: Vec<History>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "Env", default)]
    pub env: Vec<String>,
    #[serde(rename = "Cmd")]
    pub cmd: Option<Vec<String>>,
    #[serde(rename = "WorkingDir", default)]
    pub working_dir: String,
    #[serde(rename = "User", default)]
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootFs {
    #[serde(rename = "type")]
    pub fs_type: String,
    pub diff_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    pub created: String,
    pub created_by: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub empty_layer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub config: Descriptor,
    pub layers: Vec<Descriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Descriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: i64,
    pub digest: String,
}

pub struct ImageBuilder {
    binary_path: PathBuf,
    #[allow(dead_code)]
    base_image: String,
    platform: String,
}

use std::path::PathBuf;

impl ImageBuilder {
    pub fn new(binary_path: PathBuf, base_image: String, platform: String) -> Self {
        Self {
            binary_path,
            base_image,
            platform,
        }
    }

    pub async fn build(
        &self,
        registry_client: &mut crate::registry::RegistryClient,
        auth: &RegistryAuth,
    ) -> Result<(Vec<u8>, Vec<u8>, Manifest)> {
        info!("Building container image");

        let (_os, _arch) = self.parse_platform()?;

        // Fetch base image data
        info!(
            "Fetching base image: {} for platform: {}",
            self.base_image, self.platform
        );
        let (base_manifest, base_config) = registry_client
            .fetch_image_data(&self.base_image, &self.platform, auth)
            .await
            .context("Failed to fetch base image data")?;

        // Create application layer
        let (app_layer_data, app_diff_id) = self.create_layer()?;
        let app_layer_digest = format!("sha256:{}", digest(&app_layer_data));
        let app_layer_size = app_layer_data.len() as i64;

        // Combine base image layers with application layer
        let mut all_layers = Vec::new();
        for layer in &base_manifest.layers {
            all_layers.push(Descriptor {
                media_type: layer.media_type.clone(),
                size: layer.size,
                digest: layer.digest.clone(),
            });
        }

        // Add the application layer
        all_layers.push(Descriptor {
            media_type: "application/vnd.docker.image.rootfs.diff.tar.gzip".to_string(),
            size: app_layer_size,
            digest: app_layer_digest,
        });

        // Create merged config
        let config = self.create_layered_config(&base_config, &app_diff_id)?;
        let config_data = serde_json::to_vec_pretty(&config)?;
        let config_digest = format!("sha256:{}", digest(&config_data));
        let config_size = config_data.len() as i64;

        // Create manifest
        let manifest = Manifest {
            schema_version: 2,
            media_type: "application/vnd.docker.distribution.manifest.v2+json".to_string(),
            config: Descriptor {
                media_type: "application/vnd.docker.container.image.v1+json".to_string(),
                size: config_size,
                digest: config_digest,
            },
            layers: all_layers,
        };

        Ok((config_data, app_layer_data, manifest))
    }

    fn parse_platform(&self) -> Result<(String, String)> {
        let parts: Vec<&str> = self.platform.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid platform format: {}", self.platform);
        }
        Ok((parts[0].to_string(), parts[1].to_string()))
    }

    fn create_layer(&self) -> Result<(Vec<u8>, String)> {
        debug!("Creating layer from binary: {:?}", self.binary_path);

        let mut tar_data = Vec::new();
        {
            let mut tar = Builder::new(&mut tar_data);

            // Add the binary to /app/
            let mut file = File::open(&self.binary_path)?;
            let binary_name = self
                .binary_path
                .file_name()
                .context("Invalid binary path")?
                .to_str()
                .context("Invalid UTF-8 in binary name")?;

            let mut header = tar::Header::new_gnu();
            header.set_path(format!("app/{}", binary_name))?;
            header.set_size(std::fs::metadata(&self.binary_path)?.len());
            header.set_mode(0o755);
            header.set_cksum();

            tar.append(&header, &mut file)?;
            tar.finish()?;
        }

        // Calculate diff_id (digest of uncompressed tar)
        let diff_id = format!("sha256:{}", digest(&tar_data));

        // Compress the tar
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&tar_data)?;
        let compressed = encoder.finish()?;

        Ok((compressed, diff_id))
    }

    fn create_layered_config(
        &self,
        base_config: &ImageConfig,
        app_diff_id: &str,
    ) -> Result<ImageConfig> {
        let binary_name = self
            .binary_path
            .file_name()
            .context("Invalid binary path")?
            .to_str()
            .context("Invalid UTF-8 in binary name")?;

        // Merge environment variables (preserve base + add our own)
        let mut merged_env = base_config.config.env.clone();

        // Add PATH if not present
        if !merged_env.iter().any(|env| env.starts_with("PATH=")) {
            merged_env.push(
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            );
        }

        // Combine diff_ids (base layers + app layer)
        let mut merged_diff_ids = base_config.rootfs.diff_ids.clone();
        merged_diff_ids.push(app_diff_id.to_string());

        // Combine history (base history + app history)
        let mut merged_history = base_config.history.clone();
        merged_history.push(History {
            created: chrono::Utc::now().to_rfc3339(),
            created_by: "krust".to_string(),
            comment: "Built with krust".to_string(),
            empty_layer: false,
        });

        Ok(ImageConfig {
            architecture: base_config.architecture.clone(),
            os: base_config.os.clone(),
            config: Config {
                env: merged_env,
                cmd: Some(vec![format!("/app/{}", binary_name)]),
                working_dir: base_config.config.working_dir.clone(),
                user: base_config.config.user.clone(),
            },
            rootfs: RootFs {
                fs_type: "layers".to_string(),
                diff_ids: merged_diff_ids,
            },
            history: merged_history,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn create_test_binary() -> PathBuf {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"fake binary content").unwrap();
        let path = temp_file.path().to_path_buf();
        // Keep the file alive by converting to a regular file
        std::fs::copy(&path, &path).unwrap();
        path
    }

    fn create_base_image_config() -> ImageConfig {
        ImageConfig {
            architecture: "amd64".to_string(),
            os: "linux".to_string(),
            config: Config {
                env: vec![
                    "PATH=/usr/local/bin:/usr/bin:/bin".to_string(),
                    "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt".to_string(),
                ],
                cmd: None,
                working_dir: "/".to_string(),
                user: "nonroot:nonroot".to_string(),
            },
            rootfs: RootFs {
                fs_type: "layers".to_string(),
                diff_ids: vec![
                    "sha256:base_layer_1".to_string(),
                    "sha256:base_layer_2".to_string(),
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
            ],
        }
    }

    #[test]
    fn test_parse_platform() {
        let builder = ImageBuilder::new(
            PathBuf::from("/tmp/test"),
            "test-base".to_string(),
            "linux/amd64".to_string(),
        );

        let (os, arch) = builder.parse_platform().unwrap();
        assert_eq!(os, "linux");
        assert_eq!(arch, "amd64");
    }

    #[test]
    fn test_create_layered_config_preserves_base_environment() {
        let binary_path = create_test_binary();
        let builder = ImageBuilder::new(
            binary_path,
            "test-base".to_string(),
            "linux/amd64".to_string(),
        );

        let base_config = create_base_image_config();
        let app_diff_id = "sha256:app_layer_diff_id";

        let result = builder
            .create_layered_config(&base_config, app_diff_id)
            .unwrap();

        // Check that base environment variables are preserved
        assert!(result
            .config
            .env
            .contains(&"SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt".to_string()));
        assert!(result.config.env.iter().any(|env| env.starts_with("PATH=")));

        // Check that working directory is preserved
        assert_eq!(result.config.working_dir, "/");

        // Check that user is preserved
        assert_eq!(result.config.user, "nonroot:nonroot");

        // Check that architecture and OS are preserved
        assert_eq!(result.architecture, "amd64");
        assert_eq!(result.os, "linux");
    }

    #[test]
    fn test_create_layered_config_combines_diff_ids() {
        let binary_path = create_test_binary();
        let builder = ImageBuilder::new(
            binary_path,
            "test-base".to_string(),
            "linux/amd64".to_string(),
        );

        let base_config = create_base_image_config();
        let app_diff_id = "sha256:app_layer_diff_id";

        let result = builder
            .create_layered_config(&base_config, app_diff_id)
            .unwrap();

        // Check that base diff_ids are preserved and app diff_id is appended
        assert_eq!(result.rootfs.diff_ids.len(), 3);
        assert_eq!(result.rootfs.diff_ids[0], "sha256:base_layer_1");
        assert_eq!(result.rootfs.diff_ids[1], "sha256:base_layer_2");
        assert_eq!(result.rootfs.diff_ids[2], "sha256:app_layer_diff_id");
    }

    #[test]
    fn test_create_layered_config_combines_history() {
        let binary_path = create_test_binary();
        let builder = ImageBuilder::new(
            binary_path,
            "test-base".to_string(),
            "linux/amd64".to_string(),
        );

        let base_config = create_base_image_config();
        let app_diff_id = "sha256:app_layer_diff_id";

        let result = builder
            .create_layered_config(&base_config, app_diff_id)
            .unwrap();

        // Check that base history is preserved and app history is appended
        assert_eq!(result.history.len(), 3);
        assert_eq!(result.history[0].created_by, "base-image-builder");
        assert_eq!(result.history[1].created_by, "base-image-builder");
        assert_eq!(result.history[2].created_by, "krust");
        assert_eq!(result.history[2].comment, "Built with krust");
        assert!(!result.history[2].empty_layer);
    }

    #[test]
    fn test_image_config_serialization_compatibility() {
        // Test that our ImageConfig can deserialize from a realistic base image config
        let json_config = r#"{
            "architecture": "amd64",
            "os": "linux",
            "config": {
                "Env": [
                    "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                    "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt"
                ],
                "WorkingDir": "/",
                "User": "nonroot:nonroot"
            },
            "rootfs": {
                "type": "layers",
                "diff_ids": [
                    "sha256:b49b96bfa4b2477b73b0b8fe5e4ce383aa1d0026c1e845fb5d43c1f0b8bdb6ac"
                ]
            }
        }"#;

        let parsed: ImageConfig = serde_json::from_str(json_config).unwrap();

        assert_eq!(parsed.architecture, "amd64");
        assert_eq!(parsed.os, "linux");
        assert_eq!(parsed.config.env.len(), 2);
        assert_eq!(parsed.config.working_dir, "/");
        assert_eq!(parsed.config.user, "nonroot:nonroot");
        assert_eq!(parsed.rootfs.diff_ids.len(), 1);
        assert_eq!(parsed.history.len(), 0); // Default empty for missing field
    }
}
