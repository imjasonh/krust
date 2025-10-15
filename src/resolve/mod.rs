use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use yaml_rust2::{Yaml, YamlEmitter, YamlLoader};

const KRUST_PREFIX: &str = "krust://";

/// Find all krust:// references in YAML documents
pub fn find_krust_references(yaml_content: &str) -> Result<HashSet<String>> {
    let mut references = HashSet::new();

    // Parse YAML documents (handle multiple --- separated docs)
    let docs = YamlLoader::load_from_str(yaml_content)?;

    for doc in &docs {
        find_references_in_value(doc, &mut references);
    }

    Ok(references)
}

/// Recursively search for krust:// references in a YAML value
fn find_references_in_value(value: &Yaml, references: &mut HashSet<String>) {
    match value {
        Yaml::String(s) => {
            if let Some(path) = s.strip_prefix(KRUST_PREFIX) {
                references.insert(path.to_string());
            }
        }
        Yaml::Array(seq) => {
            for item in seq {
                find_references_in_value(item, references);
            }
        }
        Yaml::Hash(map) => {
            for (_key, val) in map {
                find_references_in_value(val, references);
            }
        }
        _ => {}
    }
}

/// Replace all krust:// references with resolved image digests
pub fn replace_krust_references(
    yaml_content: &str,
    replacements: &HashMap<String, String>,
) -> Result<String> {
    let mut result = Vec::new();

    // Parse and process each YAML document
    let mut docs = YamlLoader::load_from_str(yaml_content)?;

    for (i, doc) in docs.iter_mut().enumerate() {
        replace_in_value(doc, replacements);

        // Serialize back to YAML
        let mut out_str = String::new();
        let mut emitter = YamlEmitter::new(&mut out_str);
        emitter.dump(doc)?;

        // Add document separator if not the first document
        if i > 0 {
            result.push("---\n".to_string());
        }
        result.push(out_str);
    }

    Ok(result.join(""))
}

/// Recursively replace krust:// references in a YAML value
fn replace_in_value(value: &mut Yaml, replacements: &HashMap<String, String>) {
    match value {
        Yaml::String(s) => {
            if let Some(path) = s.strip_prefix(KRUST_PREFIX) {
                if let Some(replacement) = replacements.get(path) {
                    *s = replacement.clone();
                }
            }
        }
        Yaml::Array(seq) => {
            for item in seq {
                replace_in_value(item, replacements);
            }
        }
        Yaml::Hash(map) => {
            for (_key, val) in map {
                replace_in_value(val, replacements);
            }
        }
        _ => {}
    }
}

/// Read YAML files from a path (file or directory)
pub fn read_yaml_files(path: &Path) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();

    if path.is_file() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        files.push((path.display().to_string(), content));
    } else if path.is_dir() {
        // Read all .yaml and .yml files in the directory
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();

            if entry_path.is_file() {
                if let Some(ext) = entry_path.extension() {
                    if ext == "yaml" || ext == "yml" {
                        let content = std::fs::read_to_string(&entry_path)?;
                        files.push((entry_path.display().to_string(), content));
                    }
                }
            }
        }

        if files.is_empty() {
            anyhow::bail!("No YAML files found in directory: {}", path.display());
        }
    } else {
        anyhow::bail!("Path does not exist: {}", path.display());
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_krust_references() {
        let yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test
spec:
  template:
    spec:
      containers:
      - name: app
        image: krust://./example/hello-krust
      - name: sidecar
        image: krust://./example/hello-krust
"#;

        let refs = find_krust_references(yaml).unwrap();
        assert_eq!(refs.len(), 1); // Should deduplicate
        assert!(refs.contains("./example/hello-krust"));
    }

    #[test]
    fn test_find_multiple_unique_references() {
        let yaml = r#"
containers:
- image: krust://./app1
- image: krust://./app2
- image: regular-image:latest
"#;

        let refs = find_krust_references(yaml).unwrap();
        assert_eq!(refs.len(), 2);
        assert!(refs.contains("./app1"));
        assert!(refs.contains("./app2"));
    }

    #[test]
    fn test_replace_krust_references() {
        let yaml = r#"image: krust://./example/hello-krust"#;

        let mut replacements = HashMap::new();
        replacements.insert(
            "./example/hello-krust".to_string(),
            "registry.io/repo@sha256:abc123".to_string(),
        );

        let result = replace_krust_references(yaml, &replacements).unwrap();
        assert!(result.contains("registry.io/repo@sha256:abc123"));
        assert!(!result.contains("krust://"));
    }

    #[test]
    fn test_multi_document_yaml() {
        let yaml = r#"
image: krust://./app1
---
image: krust://./app2
"#;

        let refs = find_krust_references(yaml).unwrap();
        assert_eq!(refs.len(), 2);
        assert!(refs.contains("./app1"));
        assert!(refs.contains("./app2"));
    }

    #[test]
    fn test_read_yaml_files_single_file() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.yaml");
        fs::write(&file_path, "image: krust://./app").unwrap();

        let files = read_yaml_files(&file_path).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].0.contains("test.yaml"));
        assert!(files[0].1.contains("krust://./app"));
    }

    #[test]
    fn test_read_yaml_files_directory() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        fs::write(dir.path().join("test1.yaml"), "image: krust://./app1").unwrap();
        fs::write(dir.path().join("test2.yml"), "image: krust://./app2").unwrap();
        fs::write(dir.path().join("test.txt"), "not yaml").unwrap();

        let files = read_yaml_files(dir.path()).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|(name, _)| name.contains("test1.yaml")));
        assert!(files.iter().any(|(name, _)| name.contains("test2.yml")));
    }

    #[test]
    fn test_read_yaml_files_empty_directory() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let result = read_yaml_files(dir.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No YAML files found"));
    }

    #[test]
    fn test_read_yaml_files_nonexistent_path() {
        use std::path::PathBuf;

        let path = PathBuf::from("/nonexistent/path/that/does/not/exist");
        let result = read_yaml_files(&path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path does not exist"));
    }

    #[test]
    fn test_replace_references_empty_replacements() {
        let yaml = r#"image: krust://./app"#;
        let replacements = HashMap::new();

        let result = replace_krust_references(yaml, &replacements).unwrap();
        // Should keep original reference if no replacement found
        assert!(result.contains("krust://./app"));
    }
}
