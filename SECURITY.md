# Security Policy

## Supported Versions

Currently, we provide security updates for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take the security of krust seriously. If you have discovered a security vulnerability in this project, please report it to us as described below.

### Reporting Process

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, please report them via email to the project maintainers. You can find maintainer contact information in the project's Git history or by checking recent commits.

When reporting a vulnerability, please include:

- Type of issue (e.g., buffer overflow, SQL injection, cross-site scripting, etc.)
- Full paths of source file(s) related to the manifestation of the issue
- The location of the affected source code (tag/branch/commit or direct URL)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the issue, including how an attacker might exploit the issue

### Response Timeline

We will acknowledge receipt of your vulnerability report within 48 hours and will send a more detailed response within 96 hours indicating the next steps in handling your report. After the initial reply to your report, we will keep you informed of the progress towards a fix and full announcement.

### Disclosure Policy

- We will work with you to understand and verify the issue
- We will prepare a fix and release it as soon as possible
- We will credit you for the discovery (unless you prefer to remain anonymous)

## Security Best Practices

When using krust, we recommend following these security best practices:

### Container Image Security

1. **Base Image Selection**: Always use trusted, minimal base images (default: `cgr.dev/chainguard/static`)
2. **Image Scanning**: Regularly scan built images for vulnerabilities
3. **Image Signing**: Consider signing your images for verification
4. **Registry Security**: Use secure, authenticated registries for production images

### Build Security

1. **Dependency Auditing**: We use `cargo audit` in CI to check for known vulnerabilities
2. **Static Binaries**: krust builds static binaries by default, reducing runtime dependencies
3. **Minimal Attack Surface**: Built images contain only the application binary, no shell or package manager

### Supply Chain Security

1. **Dependency Management**: Keep dependencies up to date using Dependabot
2. **Build Provenance**: Consider recording build metadata for traceability
3. **SBOM Generation**: Software Bill of Materials can be generated for compliance
4. **Reproducible Builds**: Using pinned dependencies and deterministic builds when possible

## Security Features

krust includes several security-focused features:

- **Static Linking**: Builds fully static binaries using musl libc
- **Minimal Images**: Uses distroless base images with no shell or package manager
- **OCI Compliance**: Follows OCI standards for container images
- **Anonymous Registry Access**: No credentials stored or transmitted by default
- **Isolated Builds**: Each build uses a unique temporary directory

## Dependency Security

We maintain the security of our dependencies through:

- Automated security updates via Dependabot
- Regular `cargo audit` checks in CI
- Minimal dependency footprint
- Careful review of new dependencies

## Contact

For any security-related questions that don't need to be kept confidential, feel free to open an issue in the GitHub repository.
