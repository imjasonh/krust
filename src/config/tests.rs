#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.base_image, "gcr.io/distroless/static:nonroot");
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
}
