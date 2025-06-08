use serde::{Deserialize, Serialize};

/// OCI Image Index (manifest list) for multi-arch support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageIndex {
    #[serde(rename = "schemaVersion")]
    pub schema_version: i32,
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub manifests: Vec<ManifestDescriptor>,
}

/// Descriptor for a platform-specific manifest in the index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDescriptor {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    pub size: i64,
    pub digest: String,
    pub platform: Platform,
}

/// Platform information for a manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl ImageIndex {
    pub fn new(manifests: Vec<ManifestDescriptor>) -> Self {
        Self {
            schema_version: 2,
            media_type: "application/vnd.oci.image.index.v1+json".to_string(),
            manifests,
        }
    }
}
