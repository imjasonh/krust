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
}
