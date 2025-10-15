use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use tracing::{debug, error, info};

#[cfg(test)]
mod tests;

pub struct RustBuilder {
    project_path: PathBuf,
    target: String,
    cargo_args: Vec<String>,
}

pub struct BuildResult {
    pub binary_path: PathBuf,
    _temp_dir: TempDir, // Keep temp dir alive until BuildResult is dropped
}

impl RustBuilder {
    pub fn new(project_path: impl AsRef<Path>, target: &str) -> Self {
        Self {
            project_path: project_path.as_ref().to_path_buf(),
            target: target.to_string(),
            cargo_args: Vec::new(),
        }
    }

    pub fn with_cargo_args(mut self, args: Vec<String>) -> Self {
        self.cargo_args = args;
        self
    }

    pub fn build(&self) -> Result<BuildResult> {
        info!("Building Rust project at {:?}", self.project_path);

        // Use a unique target directory to avoid conflicts between concurrent builds
        let temp_target_dir =
            tempfile::tempdir().context("Failed to create temporary directory")?;
        let target_dir = temp_target_dir.path();

        let mut cmd = Command::new("cargo");
        cmd.arg("build")
            .arg("--release")
            .arg("--target")
            .arg(&self.target)
            .arg("--target-dir")
            .arg(target_dir)
            .current_dir(&self.project_path);

        // Set RUSTFLAGS for static linking
        let rustflags = if self.target.contains("musl") {
            // For musl targets, ensure fully static linking
            "-C target-feature=+crt-static"
        } else {
            // For GNU targets, link statically where possible
            "-C target-feature=+crt-static -C link-arg=-static-libgcc"
        };
        cmd.env("RUSTFLAGS", rustflags);

        // For cross-compilation on non-Linux platforms, set linker if available
        if cfg!(not(target_os = "linux")) && self.target.contains("linux") {
            // Check if we have a musl cross-compiler available
            if self.target.contains("x86_64-unknown-linux-musl") {
                // On Windows, use rust-lld with full path
                if cfg!(target_os = "windows") {
                    // Get the rust sysroot to find rust-lld
                    let sysroot_output = Command::new("rustc")
                        .arg("--print")
                        .arg("sysroot")
                        .output()
                        .context("Failed to get rustc sysroot")?;

                    if sysroot_output.status.success() {
                        let sysroot = String::from_utf8_lossy(&sysroot_output.stdout)
                            .trim()
                            .to_string();
                        let rust_lld = PathBuf::from(&sysroot)
                            .join("lib")
                            .join("rustlib")
                            .join("x86_64-pc-windows-msvc")
                            .join("bin")
                            .join("rust-lld.exe");

                        if rust_lld.exists() {
                            cmd.env(
                                "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER",
                                rust_lld.to_string_lossy().to_string(),
                            );
                            debug!("Using linker: {}", rust_lld.display());
                        } else {
                            // Fallback to just "rust-lld" and hope it's in PATH
                            cmd.env("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER", "rust-lld");
                            debug!("Using linker: rust-lld (in PATH)");
                        }
                    } else {
                        // Fallback to just "rust-lld"
                        cmd.env("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER", "rust-lld");
                        debug!("Using linker: rust-lld (fallback)");
                    }
                } else {
                    // Try common linker names on other platforms
                    let linkers = if cfg!(target_os = "macos") {
                        vec![
                            "x86_64-unknown-linux-musl-gcc",
                            "x86_64-linux-musl-gcc",
                            "musl-gcc",
                        ]
                    } else {
                        vec!["x86_64-linux-musl-gcc", "musl-gcc", "x86_64-linux-gnu-gcc"]
                    };

                    for linker in &linkers {
                        if which::which(linker).is_ok() {
                            cmd.env("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER", linker);
                            debug!("Using linker: {}", linker);
                            break;
                        }
                    }
                }
            }
        }

        for arg in &self.cargo_args {
            cmd.arg(arg);
        }

        debug!("Running command: {:?}", cmd);
        debug!("RUSTFLAGS: {}", rustflags);

        info!("Running cargo build for target: {}", self.target);
        let output = cmd.output().context("Failed to execute cargo build")?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Cargo build failed!");
            error!("stdout:\n{}", stdout);
            error!("stderr:\n{}", stderr);
            anyhow::bail!("Cargo build failed: {}", stderr);
        }

        let binary_name = self.get_binary_name()?;
        let binary_subdir = self.get_binary_subdir();
        let mut binary_path = target_dir.join(&self.target).join("release");
        if let Some(subdir) = binary_subdir {
            binary_path = binary_path.join(subdir);
        }
        binary_path = binary_path.join(&binary_name);

        // Sometimes cargo build completes but the binary isn't immediately visible
        // due to filesystem sync issues. Give it a moment.
        let mut retries = 0;
        while !binary_path.exists() && retries < 3 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            retries += 1;
        }

        if !binary_path.exists() {
            anyhow::bail!("Built binary not found at {:?}", binary_path);
        }

        info!("Successfully built binary at {:?}", binary_path);

        // Return the build result with the temp directory to keep it alive
        Ok(BuildResult {
            binary_path,
            _temp_dir: temp_target_dir,
        })
    }

    fn get_binary_name(&self) -> Result<String> {
        // Check if --example or --bin was specified
        let mut i = 0;
        while i < self.cargo_args.len() {
            if (self.cargo_args[i] == "--example" || self.cargo_args[i] == "--bin")
                && i + 1 < self.cargo_args.len()
            {
                return Ok(self.cargo_args[i + 1].clone());
            }
            i += 1;
        }

        // Fall back to package name
        let cargo_toml_path = self.project_path.join("Cargo.toml");
        let content =
            std::fs::read_to_string(&cargo_toml_path).context("Failed to read Cargo.toml")?;

        let manifest: toml::Value =
            toml::from_str(&content).context("Failed to parse Cargo.toml")?;

        let name = manifest
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .context("Failed to get package name from Cargo.toml")?;

        Ok(name.to_string())
    }

    fn get_binary_subdir(&self) -> Option<&str> {
        // Check if --example was specified (examples go in "examples/" subdir)
        for (i, arg) in self.cargo_args.iter().enumerate() {
            if arg == "--example" && i + 1 < self.cargo_args.len() {
                return Some("examples");
            }
        }
        None
    }
}

pub fn get_rust_target_triple(platform: &str) -> Result<String> {
    match platform {
        "linux/amd64" => Ok("x86_64-unknown-linux-musl".to_string()),
        "linux/arm64" => Ok("aarch64-unknown-linux-musl".to_string()),
        "linux/arm/v7" => Ok("armv7-unknown-linux-musleabihf".to_string()),
        "linux/arm/v6" => Ok("arm-unknown-linux-musleabihf".to_string()),
        "linux/386" => Ok("i686-unknown-linux-musl".to_string()),
        "linux/ppc64le" => Ok("powerpc64le-unknown-linux-musl".to_string()),
        "linux/s390x" => Ok("s390x-unknown-linux-musl".to_string()),
        "linux/riscv64" => Ok("riscv64gc-unknown-linux-musl".to_string()),
        _ => anyhow::bail!("Unsupported platform: {}", platform),
    }
}
