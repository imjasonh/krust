use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::fs::File;
use std::io::Write;
use tar::Builder;
use tracing::{debug, info};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    pub architecture: String,
    pub os: String,
    pub config: Config,
    pub rootfs: RootFs,
    pub history: Vec<History>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "Env")]
    pub env: Vec<String>,
    #[serde(rename = "Cmd")]
    pub cmd: Option<Vec<String>>,
    #[serde(rename = "WorkingDir")]
    pub working_dir: String,
    #[serde(rename = "User")]
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
    pub comment: String,
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

    pub fn build(&self) -> Result<(Vec<u8>, Vec<u8>, Manifest)> {
        info!("Building container image");

        let (os, arch) = self.parse_platform()?;

        // Create application layer
        let (layer_data, diff_id) = self.create_layer()?;
        let layer_digest = format!("sha256:{}", digest(&layer_data));
        let layer_size = layer_data.len() as i64;

        // Create image config
        let config = self.create_config(&os, &arch, &diff_id)?;
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
            layers: vec![Descriptor {
                media_type: "application/vnd.docker.image.rootfs.diff.tar.gzip".to_string(),
                size: layer_size,
                digest: layer_digest,
            }],
        };

        Ok((config_data, layer_data, manifest))
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

    fn create_config(&self, os: &str, arch: &str, layer_digest: &str) -> Result<ImageConfig> {
        let binary_name = self
            .binary_path
            .file_name()
            .context("Invalid binary path")?
            .to_str()
            .context("Invalid UTF-8 in binary name")?;

        Ok(ImageConfig {
            architecture: arch.to_string(),
            os: os.to_string(),
            config: Config {
                env: vec![
                    "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
                ],
                cmd: Some(vec![format!("/app/{}", binary_name)]),
                working_dir: "/".to_string(),
                user: "65532:65532".to_string(), // nonroot user
            },
            rootfs: RootFs {
                fs_type: "layers".to_string(),
                diff_ids: vec![layer_digest.to_string()],
            },
            history: vec![History {
                created: chrono::Utc::now().to_rfc3339(),
                created_by: "krust".to_string(),
                comment: "Built with krust".to_string(),
                empty_layer: false,
            }],
        })
    }
}
