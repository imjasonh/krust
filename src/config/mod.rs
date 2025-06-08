use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default base image for containers
    #[serde(default = "default_base_image")]
    pub base_image: String,

    /// Default registry to push images to
    pub default_registry: Option<String>,

    /// Build configuration
    #[serde(default)]
    pub build: BuildConfig,

    /// Registry authentication configuration
    #[serde(default)]
    pub registries: HashMap<String, RegistryAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildConfig {
    /// Additional environment variables for cargo build
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Default cargo build arguments
    #[serde(default)]
    pub cargo_args: Vec<String>,

    /// Target directory for build artifacts
    pub target_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryAuth {
    pub username: Option<String>,
    pub password: Option<String>,
    pub auth: Option<String>,
}

fn default_base_image() -> String {
    "cgr.dev/chainguard/static:latest".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_image: default_base_image(),
            default_registry: None,
            build: BuildConfig::default(),
            registries: HashMap::new(),
        }
    }
}

/// Project-specific configuration from Cargo.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    /// Base image for this project
    #[serde(rename = "base-image")]
    pub base_image: Option<String>,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("krust").join("config.toml");
            if config_path.exists() {
                let content = std::fs::read_to_string(config_path)?;
                let config: Config = toml::from_str(&content)?;
                return Ok(config);
            }
        }
        Ok(Config::default())
    }

    /// Load project-specific configuration from Cargo.toml
    pub fn load_project_config(project_path: &Path) -> anyhow::Result<ProjectConfig> {
        let cargo_toml_path = project_path.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            return Ok(ProjectConfig::default());
        }

        let content = std::fs::read_to_string(&cargo_toml_path)?;
        let value: toml::Value = toml::from_str(&content)?;

        // Look for [package.metadata.krust] section
        if let Some(metadata) = value
            .get("package")
            .and_then(|p| p.get("metadata"))
            .and_then(|m| m.get("krust"))
        {
            let project_config: ProjectConfig = metadata.clone().try_into()?;
            return Ok(project_config);
        }

        Ok(ProjectConfig::default())
    }
}
