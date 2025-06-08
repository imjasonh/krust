//! Example demonstrating automatic authentication with oci-distribution
//!
//! This example shows how to use the automatic auth methods that resolve
//! credentials from Docker config files and credential helpers.

use anyhow::Result;
use oci_distribution::{client::Client, Reference};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create a client
    let client = Client::default();

    // Example 1: Pull a public image (anonymous auth)
    println!("Pulling public image alpine:latest...");
    let reference: Reference = "docker.io/library/alpine:latest".parse()?;

    match client.pull_manifest_auto(&reference).await {
        Ok((manifest, digest)) => {
            println!("✓ Successfully pulled manifest");
            println!("  Digest: {}", digest);
            match manifest {
                oci_distribution::manifest::OciManifest::Image(img) => {
                    println!("  Type: Single platform image");
                    println!("  Config: {}", img.config.digest);
                }
                oci_distribution::manifest::OciManifest::ImageIndex(idx) => {
                    println!("  Type: Multi-platform image");
                    println!("  Platforms: {}", idx.manifests.len());
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to pull: {}", e);
        }
    }

    // Example 2: Get platforms for an image
    println!("\nDetecting platforms for rust:alpine...");
    let rust_ref: Reference = "docker.io/library/rust:alpine".parse()?;

    match client.get_image_platforms_auto(&rust_ref).await {
        Ok(platforms) => {
            println!("✓ Found {} platforms:", platforms.len());
            for (os, arch) in platforms {
                println!("  - {}/{}", os, arch);
            }
        }
        Err(e) => {
            println!("✗ Failed to get platforms: {}", e);
        }
    }

    // Example 3: Private registry (requires auth)
    println!("\nNote: To test with a private registry, ensure you have credentials in:");
    println!("  - ~/.docker/config.json");
    println!("  - Or DOCKER_CONFIG environment variable");
    println!("  - Or using a credential helper");

    // Uncomment to test with a private registry:
    // let private_ref: Reference = "ghcr.io/your-username/your-image:latest".parse()?;
    // match client.pull_manifest_auto(&private_ref).await {
    //     Ok((_, digest)) => println!("✓ Successfully authenticated and pulled private image: {}", digest),
    //     Err(e) => println!("✗ Failed to pull private image: {}", e),
    // }

    Ok(())
}
