use anyhow::{Context, Result};
use clap::Parser;
use krust::{
    auth::resolve_auth,
    cli::{Cli, Commands},
    config::Config,
    service::{BuildConfig, BuildService, PlatformDetector},
};
use std::path::{Path, PathBuf};
use tracing::error;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging to stderr
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    match cli.command {
        Commands::Build {
            path,
            image,
            platform,
            no_push,
            tag,
            repo,
            cargo_args,
        } => {
            let config = Config::load()?;
            let project_path = path.unwrap_or_else(|| PathBuf::from("."));

            // Load project-specific config from Cargo.toml
            let project_config = Config::load_project_config(&project_path)?;

            // Determine base image (project config takes precedence)
            let base_image = project_config
                .base_image
                .unwrap_or(config.base_image.clone());

            // Determine the target repository name
            let target_repo = determine_target_repo(image, repo, &project_path)?;

            // Determine platforms to build for
            let platforms = if let Some(platforms) = platform {
                platforms
            } else {
                // Detect platforms from base image
                let mut registry_client = krust::registry::RegistryClient::new()?;
                let base_auth = resolve_auth(&base_image)?;
                PlatformDetector::detect_platforms(&base_image, &mut registry_client, &base_auth)
                    .await?
            };

            // Build using the service layer
            let build_config = BuildConfig {
                project_path,
                base_image,
                target_repo,
                platforms,
                no_push,
                tag,
                cargo_args,
            };

            let result = BuildService::build(build_config).await?;

            // Output the image reference if it was pushed
            if let Some(image_ref) = result.image_ref {
                println!("{}", image_ref);
            }
        }
        Commands::Push { image } => {
            let _ = image;
            error!("Push command not yet implemented");
            std::process::exit(1);
        }
        Commands::Version => {
            println!("krust {}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}

fn determine_target_repo(
    image: Option<String>,
    repo: Option<String>,
    project_path: &Path,
) -> Result<String> {
    if let Some(image) = image {
        // Use explicit image if provided, strip any tag/digest
        if let Some(pos) = image.rfind([':', '@']) {
            Ok(image[..pos].to_string())
        } else {
            Ok(image)
        }
    } else {
        // Build repository name from repo and project name
        let repo = repo.context("Either --image or KRUST_REPO must be set")?;
        let project_name = get_project_name(project_path)?;
        Ok(format!("{}/{}", repo, project_name))
    }
}

fn get_project_name(project_path: &Path) -> Result<String> {
    let cargo_toml_path = project_path.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml_path).context("Failed to read Cargo.toml")?;

    let manifest: toml::Value = toml::from_str(&content).context("Failed to parse Cargo.toml")?;

    let name = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .context("Failed to get package name from Cargo.toml")?;

    Ok(name.to_string())
}
