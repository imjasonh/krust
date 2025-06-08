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
}
