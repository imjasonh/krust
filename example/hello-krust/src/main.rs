fn main() {
    println!("Hello from krust example!");
    println!("Current architecture: {}", std::env::consts::ARCH);

    // Check for SSL_CERT_FILE environment variable (common in distroless images)
    match std::env::var("SSL_CERT_FILE") {
        Ok(value) => println!("✓ SSL_CERT_FILE found: {}", value),
        Err(_) => panic!("✗ SSL_CERT_FILE not found (base image env not preserved)"),
    }

    // Check for /etc/os-release file (common in most Linux base images)
    if std::path::Path::new("/etc/os-release").exists() {
        println!("✓ /etc/os-release found (base image layers preserved)");
    } else {
        panic!("✗ /etc/os-release not found (base image layers not preserved)");
    }
}
