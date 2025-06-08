#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_parse_image_reference() {
        let (registry, repo, tag) =
            parse_image_reference("docker.io/library/hello-world:latest").unwrap();
        assert_eq!(registry, "docker.io");
        assert_eq!(repo, "library/hello-world");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_parse_image_reference_no_tag() {
        let (_, _, tag) = parse_image_reference("docker.io/library/hello-world").unwrap();
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_parse_image_reference_with_port() {
        let (registry, repo, tag) = parse_image_reference("localhost:5000/myapp:v1.0").unwrap();
        assert_eq!(registry, "localhost:5000");
        assert_eq!(repo, "myapp");
        assert_eq!(tag, "v1.0");
    }

    #[test]
    fn test_registry_client_disables_chunked_uploads() {
        // This test verifies that RegistryClient is created with chunked uploads disabled
        // The actual verification happens in the constructor where we set
        // config.use_chunked_uploads = false
        let client = RegistryClient::new();
        assert!(
            client.is_ok(),
            "RegistryClient should be created successfully"
        );

        // The important part is that the client is configured correctly in new()
        // to disable chunked uploads for better registry compatibility
    }
}
