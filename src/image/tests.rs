#[cfg(test)]
mod tests {
    use super::super::*;
    use std::path::PathBuf;

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
    fn test_parse_platform_invalid() {
        let builder = ImageBuilder::new(
            PathBuf::from("/tmp/test"),
            "test-base".to_string(),
            "invalid-platform".to_string(),
        );

        assert!(builder.parse_platform().is_err());
    }
}
