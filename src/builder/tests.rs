#[cfg(test)]
mod tests {
    use super::super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_get_rust_target_triple() {
        assert_eq!(
            get_rust_target_triple("linux/amd64").unwrap(),
            "x86_64-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple("linux/arm64").unwrap(),
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple("linux/arm/v7").unwrap(),
            "armv7-unknown-linux-musleabihf"
        );
        assert_eq!(
            get_rust_target_triple("linux/arm/v6").unwrap(),
            "arm-unknown-linux-musleabihf"
        );
        assert_eq!(
            get_rust_target_triple("linux/386").unwrap(),
            "i686-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple("linux/ppc64le").unwrap(),
            "powerpc64le-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple("linux/s390x").unwrap(),
            "s390x-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple("linux/riscv64").unwrap(),
            "riscv64gc-unknown-linux-musl"
        );
        assert!(get_rust_target_triple("windows/amd64").is_err());
    }

    #[test]
    fn test_get_binary_name_valid() {
        let dir = tempdir().unwrap();
        let cargo_toml = dir.path().join("Cargo.toml");
        fs::write(
            &cargo_toml,
            r#"
[package]
name = "test-binary"
version = "0.1.0"
"#,
        )
        .unwrap();

        let builder = RustBuilder::new(dir.path(), "x86_64-unknown-linux-musl");
        let name = builder.get_binary_name().unwrap();
        assert_eq!(name, "test-binary");
    }

    #[test]
    fn test_get_binary_name_missing_cargo_toml() {
        let dir = tempdir().unwrap();
        let builder = RustBuilder::new(dir.path(), "x86_64-unknown-linux-musl");
        let result = builder.get_binary_name();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cargo.toml"));
    }

    #[test]
    fn test_get_binary_name_invalid_toml() {
        let dir = tempdir().unwrap();
        let cargo_toml = dir.path().join("Cargo.toml");
        fs::write(&cargo_toml, "invalid toml [[[").unwrap();

        let builder = RustBuilder::new(dir.path(), "x86_64-unknown-linux-musl");
        let result = builder.get_binary_name();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("parse"));
    }

    #[test]
    fn test_get_binary_name_missing_package_name() {
        let dir = tempdir().unwrap();
        let cargo_toml = dir.path().join("Cargo.toml");
        fs::write(
            &cargo_toml,
            r#"
[package]
version = "0.1.0"
"#,
        )
        .unwrap();

        let builder = RustBuilder::new(dir.path(), "x86_64-unknown-linux-musl");
        let result = builder.get_binary_name();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("package name"));
    }

    #[test]
    fn test_rust_builder_with_cargo_args() {
        let dir = tempdir().unwrap();
        let builder = RustBuilder::new(dir.path(), "x86_64-unknown-linux-musl")
            .with_cargo_args(vec!["--features".to_string(), "foo".to_string()]);

        assert_eq!(builder.cargo_args, vec!["--features", "foo"]);
    }
}
