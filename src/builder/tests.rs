#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::constants::platform;

    #[test]
    fn test_get_rust_target_triple() {
        assert_eq!(
            get_rust_target_triple(platform::LINUX_AMD64).unwrap(),
            "x86_64-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple(platform::LINUX_ARM64).unwrap(),
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple(platform::LINUX_ARM_V7).unwrap(),
            "armv7-unknown-linux-musleabihf"
        );
        assert_eq!(
            get_rust_target_triple(platform::LINUX_ARM_V6).unwrap(),
            "arm-unknown-linux-musleabihf"
        );
        assert_eq!(
            get_rust_target_triple(platform::LINUX_386).unwrap(),
            "i686-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple(platform::LINUX_PPC64LE).unwrap(),
            "powerpc64le-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple(platform::LINUX_S390X).unwrap(),
            "s390x-unknown-linux-musl"
        );
        assert_eq!(
            get_rust_target_triple(platform::LINUX_RISCV64).unwrap(),
            "riscv64gc-unknown-linux-musl"
        );
        assert!(get_rust_target_triple("windows/amd64").is_err());
    }
}
