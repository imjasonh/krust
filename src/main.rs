use anyhow::{Context, Result};
use clap::Parser;
use krust::{
    builder::{get_rust_target_triple, RustBuilder},
    cli::{Cli, Commands},
    config::Config,
    image::ImageBuilder,
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

            // Build the Rust binary
            let target = get_rust_target_triple(&platform)?;
            let builder = RustBuilder::new(&project_path, &target).with_cargo_args(cargo_args);

            let binary_path = builder.build()?;

            // Build container image
            let image_builder =
                ImageBuilder::new(binary_path, config.base_image.clone(), platform.clone());

            let (config_data, layer_data, manifest) = image_builder.build()?;

            // Push by default unless --no-push is specified
            if !no_push {
                info!("Pushing image to registry...");
                let auth = oci_distribution::secrets::RegistryAuth::Anonymous;
                let mut registry_client = RegistryClient::new(auth)?;

                let layers = vec![(layer_data, manifest.layers[0].media_type.clone())];

                let digest_ref = registry_client
                    .push_image(&image_ref, config_data, layers)
                    .await?;

                // Print only the digest reference to stdout
                println!("{}", digest_ref);
            } else {
                info!("Successfully built image: {}", image_ref);
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
