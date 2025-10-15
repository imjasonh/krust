/// Platform constants for container images
pub mod platform {
    /// Linux AMD64 platform identifier
    pub const LINUX_AMD64: &str = "linux/amd64";
    
    /// Linux ARM64 platform identifier
    pub const LINUX_ARM64: &str = "linux/arm64";
    
    /// Linux ARMv7 platform identifier
    pub const LINUX_ARM_V7: &str = "linux/arm/v7";
    
    /// Linux ARMv6 platform identifier
    pub const LINUX_ARM_V6: &str = "linux/arm/v6";
    
    /// Linux 386 platform identifier
    pub const LINUX_386: &str = "linux/386";
    
    /// Linux PowerPC 64 LE platform identifier
    pub const LINUX_PPC64LE: &str = "linux/ppc64le";
    
    /// Linux S390X platform identifier
    pub const LINUX_S390X: &str = "linux/s390x";
    
    /// Linux RISC-V 64 platform identifier
    pub const LINUX_RISCV64: &str = "linux/riscv64";
}

/// Container image tag constants
pub mod tag {
    /// Default container image tag
    pub const DEFAULT: &str = "latest";
}

/// User and group constants
pub mod user {
    /// Nonroot user identifier
    pub const NONROOT: &str = "nonroot:nonroot";
    
    /// Nonroot user UID (used in some contexts)
    pub const NONROOT_UID: u32 = 65532;
    
    /// Nonroot user GID (used in some contexts)
    pub const NONROOT_GID: u32 = 65532;
}
