use anyhow::{Context, Result};
use clap::Parser;
use krust::{
    auth::resolve_auth,
    builder::{get_rust_target_triple, RustBuilder},
    cli::{Cli, Commands},
    config::Config,
    image::{parse_platform_string, ImageBuilder},
    manifest::{ManifestDescriptor, Platform},
    registry::RegistryClient,
    resolve::{find_krust_references, read_yaml_files, replace_krust_references},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;
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

            // Build for each platform concurrently
            let mut tasks = Vec::new();

            for platform_str in platforms.clone() {
                let project_path = project_path.clone();
                let base_image = base_image.clone();
                let target_repo = target_repo.clone();
                let cargo_args = cargo_args.clone();
                let no_push_flag = no_push;

                let task = tokio::spawn(async move {
                    let descriptor = build_and_push_platform(
                        &project_path,
                        &base_image,
                        &target_repo,
                        &platform_str,
                        cargo_args,
                        !no_push_flag,
                    )
                    .await?;

                    Ok::<_, anyhow::Error>(descriptor)
                });

                tasks.push(task);
            }

            // Wait for all builds to complete
            let mut manifest_descriptors = Vec::new();
            for task in tasks {
                let result = task.await.context("Build task panicked")??;
                if let Some(descriptor) = result {
                    manifest_descriptors.push(descriptor);
                }
            }

            // Always push manifest list if not --no-push (even for single platform)
            if !no_push {
                let image_ref = push_tagged_manifest_list(
                    &mut registry_client,
                    &target_repo,
                    manifest_descriptors,
                    &tag,
                )
                .await?;

                // Output the manifest list reference (always by digest)
                println!("{}", image_ref);
            } else {
                info!(
                    "Successfully built image for {} platform(s)",
                    platforms.len()
                );
                info!("Skipping push (--no-push specified)");
            }
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

/// Build a binary and push an image for a single platform.
/// Returns a ManifestDescriptor if push is true, None otherwise.
async fn build_and_push_platform(
    project_path: &Path,
    base_image: &str,
    target_repo: &str,
    platform_str: &str,
    cargo_args: Vec<String>,
    push: bool,
) -> Result<Option<ManifestDescriptor>> {
    info!("Building for platform: {}", platform_str);

    // Build the Rust binary for this platform
    let target = get_rust_target_triple(platform_str)?;
    let builder = RustBuilder::new(project_path, &target).with_cargo_args(cargo_args);
    let build_result = builder.build()?;

    // Build container image for this platform
    let image_builder = ImageBuilder::new(
        build_result.binary_path,
        base_image.to_string(),
        platform_str.to_string(),
    );

    // Create a registry client for this task
    let mut registry_client = RegistryClient::new()?;

    let base_auth = resolve_auth(base_image)?;
    let (config_data, layer_data, manifest) = image_builder
        .build(&mut registry_client, &base_auth)
        .await?;

    if !push {
        return Ok(None);
    }

    info!("Pushing image for platform: {}", platform_str);

    let push_auth = resolve_auth(target_repo)?;
    let app_layer_media_type = manifest
        .layers
        .last()
        .map(|l| l.media_type.clone())
        .unwrap_or_else(|| "application/vnd.oci.image.layer.v1.tar+gzip".to_string());

    let (digest_ref, manifest_size) = registry_client
        .push_layered_image(
            target_repo,
            config_data,
            layer_data,
            app_layer_media_type,
            &manifest,
            &push_auth,
            base_image,
            &base_auth,
        )
        .await?;

    let (os, arch, variant) = parse_platform_string(platform_str)?;
    let digest = digest_ref.split('@').next_back().unwrap_or("").to_string();

    info!("Pushed platform image: {} ({})", digest_ref, platform_str);

    Ok(Some(ManifestDescriptor {
        media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
        size: manifest_size as i64,
        digest,
        platform: Platform {
            architecture: arch,
            os,
            variant,
        },
    }))
}

/// Push a manifest list, optionally tagged.
async fn push_tagged_manifest_list(
    registry_client: &mut RegistryClient,
    target_repo: &str,
    manifest_descriptors: Vec<ManifestDescriptor>,
    tag: &Option<String>,
) -> Result<String> {
    info!("Creating and pushing manifest list...");

    let has_tag = tag.is_some();
    let manifest_target = if let Some(tag_name) = tag {
        format!("{}:{}", target_repo, tag_name)
    } else {
        target_repo.to_string()
    };

    let final_auth = resolve_auth(&manifest_target)?;

    registry_client
        .push_manifest_list(&manifest_target, manifest_descriptors, &final_auth, has_tag)
        .await
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

        let project_path = PathBuf::from(&krust_path);
        if !project_path.exists() {
            anyhow::bail!("Path does not exist: {}", krust_path);
        }

        let project_name = get_project_name(&project_path)?;
        let target_repo = format!("{}/{}", repo, project_name);

        let project_config = Config::load_project_config(&project_path)?;
        let base_image = project_config
            .base_image
            .unwrap_or(config.base_image.clone());

        let platforms = if let Some(ref platforms) = platform {
            platforms.clone()
        } else {
            vec!["linux/amd64".to_string()]
        };

        // Build for each platform
        let mut manifest_descriptors = Vec::new();
        for platform_str in &platforms {
            if let Some(descriptor) = build_and_push_platform(
                &project_path,
                &base_image,
                &target_repo,
                platform_str,
                Vec::new(),
                true,
            )
            .await?
            {
                manifest_descriptors.push(descriptor);
            }
        }

        // Push manifest list
        let image_ref = push_tagged_manifest_list(
            &mut registry_client,
            &target_repo,
            manifest_descriptors,
            &tag,
        )
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
