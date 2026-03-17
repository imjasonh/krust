use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

#[cfg(test)]
mod tests;

pub struct RustBuilder {
    project_path: PathBuf,
    target: String,
    cargo_args: Vec<String>,
}

pub struct BuildResult {
    pub binary_path: PathBuf,
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

    /// Check if cargo-zigbuild is available.
    fn has_zigbuild() -> bool {
        Command::new("cargo")
            .args(["zigbuild", "--version"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check if the rustup target is installed, and install it if not.
    fn ensure_target_installed(target: &str) -> Result<()> {
        let output = Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output()
            .context("Failed to run rustup. Is rustup installed?")?;

        let installed = String::from_utf8_lossy(&output.stdout);
        if installed.lines().any(|line| line.trim() == target) {
            return Ok(());
        }

        info!("Installing rustup target: {}", target);
        let status = Command::new("rustup")
            .args(["target", "add", target])
            .status()
            .context("Failed to run rustup target add")?;

        if !status.success() {
            anyhow::bail!(
                "Failed to install target '{}'. Run: rustup target add {}",
                target,
                target
            );
        }

        Ok(())
    }

    /// Get the persistent target directory for krust builds.
    /// Uses `<project>/target/krust/` so cargo can reuse build caches.
    fn target_dir(&self) -> PathBuf {
        self.project_path.join("target").join("krust")
    }

    pub fn build(&self) -> Result<BuildResult> {
        info!("Building Rust project at {:?}", self.project_path);

        // Ensure the target is installed via rustup
        Self::ensure_target_installed(&self.target)?;

        let target_dir = self.target_dir();
        let use_zigbuild = Self::has_zigbuild();

        let mut cmd = Command::new("cargo");
        if use_zigbuild {
            info!("Using cargo-zigbuild for cross-compilation");
            cmd.arg("zigbuild");
        } else {
            warn!(
                "cargo-zigbuild not found, falling back to cargo build. \
                 Install it for easier cross-compilation: cargo install cargo-zigbuild"
            );
            cmd.arg("build");
        }

        cmd.arg("--release")
            .arg("--target")
            .arg(&self.target)
            .arg("--target-dir")
            .arg(&target_dir)
            .current_dir(&self.project_path);

        // Set RUSTFLAGS for static linking
        let rustflags = if self.target.contains("musl") {
            "-C target-feature=+crt-static"
        } else {
            "-C target-feature=+crt-static -C link-arg=-static-libgcc"
        };
        cmd.env("RUSTFLAGS", rustflags);

        // When not using zigbuild, try to find a cross-linker for non-native targets
        if !use_zigbuild {
            self.configure_cross_linker(&mut cmd);
        }

        for arg in &self.cargo_args {
            cmd.arg(arg);
        }

        debug!("Running command: {:?}", cmd);
        debug!("RUSTFLAGS: {}", rustflags);

        info!("Running cargo build for target: {}", self.target);
        let output = cmd.output().context("Failed to execute cargo build")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Provide actionable error messages
            let stderr_str = stderr.to_string();
            if stderr_str.contains("linker") && stderr_str.contains("not found") {
                anyhow::bail!(
                    "Cross-compilation linker not found for target '{}'.\n\
                     Install cargo-zigbuild for easy cross-compilation:\n\
                     \n  cargo install cargo-zigbuild\n\
                     \nOr install the appropriate system linker for your platform.",
                    self.target
                );
            }

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

        Ok(BuildResult { binary_path })
    }

    /// Configure cross-compilation linker when not using zigbuild.
    /// This is a best-effort fallback for users who don't have zigbuild installed.
    fn configure_cross_linker(&self, cmd: &mut Command) {
        // Only needed when cross-compiling
        let is_cross = if cfg!(target_os = "linux") {
            // On Linux, check if we're building for a different arch
            let host_arch = std::env::consts::ARCH;
            let target_arch = self.target.split('-').next().unwrap_or("");
            host_arch != target_arch
                && !(host_arch == "x86_64" && target_arch == "x86_64")
                && !(host_arch == "aarch64" && target_arch == "aarch64")
        } else {
            // On non-Linux hosts, always need cross-linker for Linux targets
            self.target.contains("linux")
        };

        if !is_cross {
            return;
        }

        // Build the CARGO_TARGET_<triple>_LINKER env var name
        let env_var = format!(
            "CARGO_TARGET_{}_LINKER",
            self.target.to_uppercase().replace('-', "_")
        );

        // Try common linker names based on target
        let linkers: Vec<&str> = if self.target.contains("x86_64") && self.target.contains("musl") {
            vec!["x86_64-linux-musl-gcc", "musl-gcc"]
        } else if self.target.contains("aarch64") && self.target.contains("musl") {
            vec!["aarch64-linux-musl-gcc", "aarch64-linux-gnu-gcc"]
        } else if self.target.contains("armv7") {
            vec!["arm-linux-gnueabihf-gcc", "armv7-linux-musleabihf-gcc"]
        } else if self.target.contains("arm-") {
            vec!["arm-linux-gnueabihf-gcc", "arm-linux-musleabihf-gcc"]
        } else if self.target.contains("i686") {
            vec!["i686-linux-musl-gcc", "i686-linux-gnu-gcc"]
        } else if self.target.contains("powerpc64le") {
            vec!["powerpc64le-linux-gnu-gcc"]
        } else if self.target.contains("s390x") {
            vec!["s390x-linux-gnu-gcc"]
        } else if self.target.contains("riscv64") {
            vec!["riscv64-linux-gnu-gcc"]
        } else {
            vec![]
        };

        for linker in &linkers {
            if which::which(linker).is_ok() {
                cmd.env(&env_var, linker);
                debug!("Using linker: {}", linker);
                return;
            }
        }

        if !linkers.is_empty() {
            debug!(
                "No cross-linker found for target '{}'. Build may fail. \
                 Consider installing cargo-zigbuild: cargo install cargo-zigbuild",
                self.target
            );
        }
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
