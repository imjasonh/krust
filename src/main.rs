use anyhow::{Context, Result};
use clap::Parser;
use krust::{
    auth::resolve_auth,
    builder::{get_rust_target_triple, RustBuilder},
    cli::{Cli, Commands},
    config::Config,
    image::ImageBuilder,
    manifest::{ManifestDescriptor, Platform},
    registry::RegistryClient,
    resolve::{find_krust_references, read_yaml_files, replace_krust_references},
};
use std::collections::HashMap;
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

            // Build repository name from KRUST_REPO and project name
            let repo = repo.context("KRUST_REPO must be set")?;
            let project_name = get_project_name(&project_path)?;
            let target_repo = format!("{}/{}", repo, project_name);

            // Initialize registry client
            let mut registry_client = RegistryClient::new()?;

            // Determine platforms to build for
            let platforms = if let Some(platforms) = platform {
                // Use explicitly specified platforms
                platforms
            } else {
                // Detect platforms from base image
                info!(
                    "Detecting available platforms from base image: {}",
                    base_image
                );
                // Get auth for the base image registry
                let base_auth = resolve_auth(&base_image)?;

                match registry_client
                    .get_image_platforms(&base_image, &base_auth)
                    .await
                {
                    Ok(detected_platforms) => {
                        if detected_platforms.is_empty() {
                            info!("No platforms detected, using defaults");
                            vec!["linux/amd64".to_string(), "linux/arm64".to_string()]
                        } else {
                            info!("Detected platforms: {:?}", detected_platforms);
                            detected_platforms
                        }
                    }
                    Err(e) => {
                        info!("Failed to detect platforms: {}. Using defaults.", e);
                        vec!["linux/amd64".to_string(), "linux/arm64".to_string()]
                    }
                }
            };

            // Build for each platform
            let mut manifest_descriptors = Vec::new();

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

                // Always use layered approach - registry layer will handle cross-registry blob copying
                let base_auth = resolve_auth(&base_image)?;
                let (config_data, layer_data, manifest) = image_builder
                    .build(&mut registry_client, &base_auth)
                    .await?;

                // Push platform-specific image if not --no-push
                if !no_push {
                    info!("Pushing image for platform: {}", platform_str);

                    // Get auth for the target registry
                    let push_auth = resolve_auth(&target_repo)?;

                    // Get the media type of the application layer (last layer in manifest)
                    let app_layer_media_type = manifest
                        .layers
                        .last()
                        .map(|l| l.media_type.clone())
                        .unwrap_or_else(|| {
                            "application/vnd.docker.image.rootfs.diff.tar.gzip".to_string()
                        });

                    // Push layered image by digest only (no tag)
                    // This will be referenced by digest in the final manifest list
                    let (digest_ref, manifest_size) = registry_client
                        .push_layered_image(
                            &target_repo,
                            config_data,
                            layer_data,
                            app_layer_media_type,
                            &manifest,
                            &push_auth,
                            &base_image,
                            &base_auth,
                        )
                        .await?;

                    // Parse platform string
                    let parts: Vec<&str> = platform_str.split('/').collect();
                    let (os, arch) = if parts.len() >= 2 {
                        (parts[0].to_string(), parts[1].to_string())
                    } else {
                        return Err(anyhow::anyhow!("Invalid platform format: {}", platform_str));
                    };

                    // Extract just the digest from the full reference
                    let digest = digest_ref.split('@').next_back().unwrap_or("").to_string();

                    info!("Pushed platform image to: {}", digest_ref);

                    // Add to manifest list
                    info!(
                        "Adding manifest to list - platform: {}/{}, digest: {}, size: {}",
                        os, arch, digest, manifest_size
                    );
                    manifest_descriptors.push(ManifestDescriptor {
                        media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
                        size: manifest_size as i64,
                        digest,
                        platform: Platform {
                            architecture: arch,
                            os,
                            variant: None,
                        },
                    });
                }
            }

            // Always push manifest list if not --no-push (even for single platform)
            if !no_push {
                info!("Creating and pushing manifest list...");

                // Determine the target for the manifest list
                let has_tag = tag.is_some();
                let manifest_target = if let Some(tag_name) = &tag {
                    // If --tag is specified, push to that tag
                    format!("{}:{}", target_repo, tag_name)
                } else {
                    // If no tag specified, push digest-only (no tag)
                    target_repo.clone()
                };

                // Get auth for the final image push
                let final_auth = resolve_auth(&manifest_target)?;

                let manifest_list_ref = registry_client
                    .push_manifest_list(
                        &manifest_target,
                        manifest_descriptors,
                        &final_auth,
                        has_tag,
                    )
                    .await?;

                // Output the manifest list reference (always by digest)
                println!("{}", manifest_list_ref);
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
        Commands::Resolve {
            filenames,
            platform,
            repo,
            tag,
        } => {
            let resolved_yaml = resolve_yaml_files(filenames, platform, repo, tag).await?;

            // Output all documents separated by ---
            for (i, doc) in resolved_yaml.iter().enumerate() {
                if i > 0 {
                    println!("---");
                }
                print!("{}", doc);
            }
        }
        Commands::Apply {
            filenames,
            platform,
            repo,
            tag,
        } => {
            let resolved_yaml = resolve_yaml_files(filenames, platform, repo, tag).await?;

            // Combine all documents and pipe to kubectl
            let combined_yaml = resolved_yaml.join("---\n");

            // Execute kubectl apply
            let mut kubectl = std::process::Command::new("kubectl")
                .args(["apply", "-f", "-"])
                .stdin(std::process::Stdio::piped())
                .spawn()
                .context("Failed to execute kubectl - is it installed?")?;

            // Write YAML to kubectl's stdin
            if let Some(mut stdin) = kubectl.stdin.take() {
                use std::io::Write;
                stdin
                    .write_all(combined_yaml.as_bytes())
                    .context("Failed to write to kubectl stdin")?;
            }

            // Wait for kubectl to finish
            let status = kubectl.wait()?;

            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Commands::Version => {
            println!("krust {}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}

/// Resolve krust:// references in YAML files
async fn resolve_yaml_files(
    filenames: Vec<PathBuf>,
    platform: Option<Vec<String>>,
    repo: Option<String>,
    tag: Option<String>,
) -> Result<Vec<String>> {
    let repo = repo.context("KRUST_REPO must be set")?;
    let config = Config::load()?;

    // Collect all YAML content and find all krust:// references
    let mut all_yaml_files = Vec::new();
    let mut all_references = std::collections::HashSet::new();

    for path in &filenames {
        let yaml_files = read_yaml_files(path)?;
        for (filename, content) in &yaml_files {
            let refs = find_krust_references(content)?;
            all_references.extend(refs);
            all_yaml_files.push((filename.clone(), content.clone()));
        }
    }

    info!(
        "Found {} unique krust:// reference(s)",
        all_references.len()
    );

    // Build and push images for each unique reference
    let mut replacements = HashMap::new();
    let mut registry_client = RegistryClient::new()?;

    for krust_path in all_references {
        info!("Building image for: krust://{}", krust_path);

        // Resolve the path (could be relative like ./example/hello-krust)
        let project_path = PathBuf::from(&krust_path);

        if !project_path.exists() {
            anyhow::bail!("Path does not exist: {}", krust_path);
        }

        // Get project name
        let project_name = get_project_name(&project_path)?;
        let target_repo = format!("{}/{}", repo, project_name);

        // Load project config
        let project_config = Config::load_project_config(&project_path)?;
        let base_image = project_config
            .base_image
            .unwrap_or(config.base_image.clone());

        // Determine platforms
        let platforms = if let Some(ref platforms) = platform {
            platforms.clone()
        } else {
            // Default to linux/amd64 for resolve
            vec!["linux/amd64".to_string()]
        };

        // Build for each platform
        let mut manifest_descriptors = Vec::new();

        for platform_str in &platforms {
            info!("Building {} for platform: {}", krust_path, platform_str);

            let target = get_rust_target_triple(platform_str)?;
            let builder = RustBuilder::new(&project_path, &target);
            let build_result = builder.build()?;

            let image_builder = ImageBuilder::new(
                build_result.binary_path,
                base_image.clone(),
                platform_str.clone(),
            );

            let base_auth = resolve_auth(&base_image)?;
            let (config_data, layer_data, manifest) = image_builder
                .build(&mut registry_client, &base_auth)
                .await?;

            // Push the image
            let push_auth = resolve_auth(&target_repo)?;
            let app_layer_media_type = manifest
                .layers
                .last()
                .map(|l| l.media_type.clone())
                .unwrap_or_else(|| "application/vnd.docker.image.rootfs.diff.tar.gzip".to_string());

            let (digest_ref, manifest_size) = registry_client
                .push_layered_image(
                    &target_repo,
                    config_data,
                    layer_data,
                    app_layer_media_type,
                    &manifest,
                    &push_auth,
                    &base_image,
                    &base_auth,
                )
                .await?;

            // Parse platform
            let parts: Vec<&str> = platform_str.split('/').collect();
            let (os, arch) = if parts.len() >= 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                return Err(anyhow::anyhow!("Invalid platform format: {}", platform_str));
            };

            let digest = digest_ref.split('@').next_back().unwrap_or("").to_string();

            manifest_descriptors.push(ManifestDescriptor {
                media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
                size: manifest_size as i64,
                digest,
                platform: Platform {
                    architecture: arch,
                    os,
                    variant: None,
                },
            });
        }

        // Push manifest list
        let has_tag = tag.is_some();
        let manifest_target = if let Some(tag_name) = &tag {
            format!("{}:{}", target_repo, tag_name)
        } else {
            target_repo.clone()
        };

        let final_auth = resolve_auth(&manifest_target)?;
        let image_ref = registry_client
            .push_manifest_list(&manifest_target, manifest_descriptors, &final_auth, has_tag)
            .await?;

        info!("Resolved krust://{} -> {}", krust_path, image_ref);
        replacements.insert(krust_path, image_ref);
    }

    // Replace references in all YAML files and return resolved docs
    let mut output_docs = Vec::new();

    for (filename, content) in &all_yaml_files {
        info!("Resolving references in: {}", filename);
        let resolved = replace_krust_references(content, &replacements)?;
        output_docs.push(resolved);
    }

    Ok(output_docs)
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
