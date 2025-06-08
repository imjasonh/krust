use oci_distribution::client::{ClientConfig, Client};
use oci_distribution::secrets::RegistryAuth;
use oci_distribution::Reference;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("oci_distribution=debug".parse()?)
        )
        .init();

    // Create a client
    let config = ClientConfig::default();
    let client = Client::new(config);

    // Example 1: Using the new from_default() method
    println!("Example 1: Using RegistryAuth::from_default()");
    let alpine_ref = Reference::try_from("docker.io/library/alpine:latest")?;
    let auth = RegistryAuth::from_default(&alpine_ref)?;
    match client.pull_manifest(&alpine_ref, &auth).await {
        Ok((manifest, digest)) => {
            println!("Successfully pulled manifest for alpine:latest");
            println!("Digest: {}", digest);

            // Handle the manifest enum
            match manifest {
                oci_distribution::manifest::OciManifest::Image(img) => {
                    println!("Config digest: {}", img.config.digest);
                }
                oci_distribution::manifest::OciManifest::ImageIndex(_) => {
                    println!("Got an image index manifest");
                }
            }
        }
        Err(e) => {
            println!("Failed to pull alpine:latest: {}", e);
        }
    }

    // Example 2: Using the convenience *_auto methods (same result, less code)
    println!("\nExample 2: Using pull_manifest_auto() convenience method");
    let busybox_ref = Reference::try_from("docker.io/library/busybox:latest")?;
    match client.pull_manifest_auto(&busybox_ref).await {
        Ok((manifest, digest)) => {
            println!("Successfully pulled manifest for busybox:latest");
            println!("Digest: {}", digest);
        }
        Err(e) => {
            println!("Failed to pull busybox:latest: {}", e);
        }
    }

    // Example 3: Get platforms using auto auth
    println!("\nExample 3: Getting platforms with automatic auth");
    match client.get_image_platforms_auto(&busybox_ref).await {
        Ok(platforms) => {
            println!("Busybox platforms:");
            for (arch, os) in platforms {
                println!("  - {}/{}", os, arch);
            }
        }
        Err(e) => {
            println!("Failed to get platforms: {}", e);
        }
    }

    // Example 4: Using from_default_str for non-Reference strings
    println!("\nExample 4: Using RegistryAuth::from_default_str()");
    let auth = RegistryAuth::from_default_str("gcr.io/example/image:tag")?;
    println!("Resolved auth for gcr.io: {:?}", auth);

    // If you have credentials configured for private registries,
    // both approaches will automatically use them:
    //
    // let private_ref = Reference::try_from("ghcr.io/myuser/myimage:latest")?;
    //
    // // Option 1: Explicit auth resolution
    // let auth = RegistryAuth::from_default(&private_ref)?;
    // let result = client.pull(&private_ref, &auth, vec![]).await?;
    //
    // // Option 2: Using the convenience method
    // let result = client.pull_auto(&private_ref).await?;

    Ok(())
}
