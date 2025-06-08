#[cfg(test)]
mod tests {
    use super::super::*;

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
        assert!(get_rust_target_triple("windows/amd64").is_err());
    }
}
