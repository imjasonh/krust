#[cfg(test)]
mod tests {
    use super::super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.base_image, "cgr.dev/chainguard/static:latest");
        assert!(config.default_registry.is_none());
        assert!(config.registries.is_empty());
    }

    #[test]
    fn test_build_config_default() {
        let build_config = BuildConfig::default();
        assert!(build_config.env.is_empty());
        assert!(build_config.cargo_args.is_empty());
        assert!(build_config.target_dir.is_none());
    }

    #[test]
    fn test_load_project_config_no_cargo_toml() {
        let dir = tempdir().unwrap();
        let config = Config::load_project_config(dir.path()).unwrap();
        assert!(config.base_image.is_none());
    }

    #[test]
    fn test_load_project_config_with_metadata() {
        let dir = tempdir().unwrap();
        let cargo_toml = dir.path().join("Cargo.toml");
        fs::write(
            &cargo_toml,
            r#"
[package]
name = "test"
version = "0.1.0"

[package.metadata.krust]
base-image = "custom:latest"
"#,
        )
        .unwrap();

        let config = Config::load_project_config(dir.path()).unwrap();
        assert_eq!(config.base_image, Some("custom:latest".to_string()));
    }

    #[test]
    fn test_load_project_config_without_metadata() {
        let dir = tempdir().unwrap();
        let cargo_toml = dir.path().join("Cargo.toml");
        fs::write(
            &cargo_toml,
            r#"
[package]
name = "test"
version = "0.1.0"
"#,
        )
        .unwrap();

        let config = Config::load_project_config(dir.path()).unwrap();
        assert!(config.base_image.is_none());
    }

    #[test]
    fn test_load_project_config_invalid_toml() {
        let dir = tempdir().unwrap();
        let cargo_toml = dir.path().join("Cargo.toml");
        fs::write(&cargo_toml, "invalid toml [[[").unwrap();

        let result = Config::load_project_config(dir.path());
        assert!(result.is_err());
    }
}
