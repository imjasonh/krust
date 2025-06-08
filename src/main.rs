use anyhow::{Context, Result};
use clap::Parser;
use krust::{
    builder::{get_rust_target_triple, RustBuilder},
    cli::{Cli, Commands},
    config::Config,
    image::ImageBuilder,
    manifest::{ImageIndex, ManifestDescriptor, Platform},
    registry::RegistryClient,
};
use std::path::{Path, PathBuf};
use tracing::{error, info};
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

            // Determine the image name
            let image_ref = if let Some(image) = image {
                // Use explicit image if provided
                image
            } else {
                // Build image name from repo and project name
                let repo = repo.context("Either --image or KRUST_REPO must be set")?;
                let project_name = get_project_name(&project_path)?;
                format!("{}/{}:latest", repo, project_name)
            };

            // Determine platforms to build for
            let platforms = if let Some(platforms) = platform {
                platforms
            } else {
                // Default to common platforms
                vec!["linux/amd64".to_string(), "linux/arm64".to_string()]
            };

            // Build for each platform
            let mut manifest_descriptors = Vec::new();
            let mut single_platform_digest = None;
            let auth = oci_distribution::secrets::RegistryAuth::Anonymous;
            let mut registry_client = RegistryClient::new(auth)?;

            for platform_str in &platforms {
                info!("Building for platform: {}", platform_str);

                // Build the Rust binary for this platform
                let target = get_rust_target_triple(platform_str)?;
                let builder =
                    RustBuilder::new(&project_path, &target).with_cargo_args(cargo_args.clone());

                let build_result = builder.build()?;

                // Build container image for this platform
                let image_builder = ImageBuilder::new(
                    build_result.binary_path,
                    base_image.clone(),
                    platform_str.clone(),
                );

                let (config_data, layer_data, manifest) = image_builder.build()?;

                // Push platform-specific image if not --no-push
                if !no_push {
                    info!("Pushing image for platform: {}", platform_str);

                    let layers = vec![(layer_data, manifest.layers[0].media_type.clone())];

                    // For single platform, push to the main tag
                    let push_ref = if platforms.len() == 1 {
                        image_ref.clone()
                    } else {
                        // For multi-platform, use platform-specific tag
                        format!("{}-{}", image_ref, platform_str.replace('/', "-"))
                    };

                    let digest_ref = registry_client
                        .push_image(&push_ref, config_data, layers)
                        .await?;

                    // Store digest for single platform builds
                    if platforms.len() == 1 {
                        single_platform_digest = Some(digest_ref.clone());
                    }

                    // Parse platform string
                    let parts: Vec<&str> = platform_str.split('/').collect();
                    let (os, arch) = if parts.len() >= 2 {
                        (parts[0].to_string(), parts[1].to_string())
                    } else {
                        return Err(anyhow::anyhow!("Invalid platform format: {}", platform_str));
                    };

                    // Add to manifest list
                    manifest_descriptors.push(ManifestDescriptor {
                        media_type: manifest.media_type.clone(),
                        size: serde_json::to_vec(&manifest)?.len() as i64,
                        digest: digest_ref.split('@').next_back().unwrap_or("").to_string(),
                        platform: Platform {
                            architecture: arch,
                            os,
                            variant: None,
                        },
                    });
                }
            }

            // Create and push manifest list if not --no-push
            if !no_push && platforms.len() > 1 {
                info!("Creating and pushing manifest list...");
                let index = ImageIndex::new(manifest_descriptors);
                let _index_data = serde_json::to_vec(&index)?;

                // TODO: Push manifest list to registry
                // For now, just print the multi-arch image reference
                println!("{}", image_ref);
            } else if !no_push && platforms.len() == 1 {
                // Single platform, print the digest reference
                if let Some(digest_ref) = single_platform_digest {
                    println!("{}", digest_ref);
                }
            } else {
                info!(
                    "Successfully built image for {} platform(s)",
                    platforms.len()
                );
                info!("Skipping push (--no-push specified)");
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
