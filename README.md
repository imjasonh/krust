# krust

[![CI](https://github.com/imjasonh/krust/actions/workflows/ci.yml/badge.svg)](https://github.com/imjasonh/krust/actions/workflows/ci.yml)

A container image build tool for Rust applications, inspired by [`ko`](https://ko.build) for Go.

## Overview

krust builds container images for Rust applications without requiring Docker. It:
- Executes `cargo build` to compile your Rust application as a static binary using musl libc
- Packages the resulting binary into a minimal container image layer
- Pushes images to OCI-compliant registries by default (use `--no-push` to skip)
- Creates truly static binaries by default for maximum portability and security

## Quick Start

```bash
# Install krust
cargo install --path .

# Set up your repository
export KRUST_REPO=<repository-to-push-to>

# Build and run your Rust app as a container
docker run $(krust build)
```

## Installation

```bash
cargo install --path .
```

### Prerequisites

Install the Linux musl targets for static binary cross-compilation:

```bash
# For linux/amd64 (most common)
rustup target add x86_64-unknown-linux-musl

# For linux/arm64
rustup target add aarch64-unknown-linux-musl

# Or install all supported targets at once
rustup target add \
    x86_64-unknown-linux-musl \
    aarch64-unknown-linux-musl \
    armv7-unknown-linux-musleabihf \
    arm-unknown-linux-musleabihf \
    i686-unknown-linux-musl \
    powerpc64le-unknown-linux-musl \
    s390x-unknown-linux-musl \
    riscv64gc-unknown-linux-musl
```

#### macOS Cross-compilation Setup

On macOS, you'll need a cross-compilation toolchain:

```bash
# Install musl cross-compilation tools
brew install filosottile/musl-cross/musl-cross

# Note: The musl-cross formula typically only includes x86_64 and aarch64 toolchains.
# For other architectures, you may need additional toolchains or use Docker/remote builders.

# Create a .cargo/config.toml in your project with:
cat > .cargo/config.toml << 'EOF'
[target.x86_64-unknown-linux-musl]
linker = "x86_64-linux-musl-gcc"

[target.aarch64-unknown-linux-musl]
linker = "aarch64-linux-musl-gcc"

# For other architectures, you'll need to install the appropriate cross-compiler
# or use cargo-zigbuild which can target all platforms:
# cargo install cargo-zigbuild
# Then build with: cargo zigbuild --target <target>
EOF
```

Note: krust builds fully static binaries by default using musl libc, ensuring maximum portability across different Linux distributions and container environments.

## Usage

krust outputs the pushed image reference by digest to stdout, with all other output going to stderr. This enables composability with other tools.

### Build a project in the current directory

```bash
# Set your repository prefix
export KRUST_REPO=<repository-to-push-to>

# Build and push (default behavior)
krust build

# Build without pushing
krust build --no-push

# Build, push, and run immediately
docker run $(krust build)
```

### Build a specific directory

```bash
# Build and push a specific project
krust build path/to/rust/project

# Build without pushing
krust build example/hello-krust --no-push
```

### Override the image name

```bash
# Use a specific image name (overrides KRUST_REPO)
krust build --image myregistry.io/myapp:v1.0

# Build for a specific platform
krust build --platform linux/arm64

# Build for multiple platforms (multi-arch)
krust build --platform linux/amd64,linux/arm64

# Or specify platforms separately
krust build --platform linux/amd64 --platform linux/arm64

# Default behavior detects platforms from base image
# If the base image supports multiple platforms, krust will build for all of them
krust build
```

### Build with custom cargo arguments

```bash
krust build -- --features=prod
```

## Supported Platforms

- `linux/amd64` (x86_64-unknown-linux-musl)
- `linux/arm64` (aarch64-unknown-linux-musl)
- `linux/arm/v7` (armv7-unknown-linux-musleabihf)
- `linux/arm/v6` (arm-unknown-linux-musleabihf)
- `linux/386` (i686-unknown-linux-musl)
- `linux/ppc64le` (powerpc64le-unknown-linux-musl)
- `linux/s390x` (s390x-unknown-linux-musl)
- `linux/riscv64` (riscv64gc-unknown-linux-musl)

### Multi-Architecture Images

krust always pushes OCI image indexes (manifest lists) for consistency:
1. Builds each platform separately with its own binary
2. Pushes platform-specific images with unique tags
3. Creates and pushes a manifest list that references all platforms
4. Returns the manifest list digest for use with Docker/Kubernetes

This means even single-platform builds result in a manifest list, ensuring a uniform interface regardless of the number of platforms built.

#### Automatic Platform Detection

When you don't specify `--platform`, krust automatically detects which platforms to build for by inspecting the base image:

```bash
# If using cgr.dev/chainguard/static:latest (supports linux/amd64 and linux/arm64)
krust build  # Automatically builds for both amd64 and arm64

# If using a single-platform base image
krust build  # Builds only for the supported platform

# You can always override with explicit platforms
krust build --platform linux/amd64  # Build only for amd64 regardless of base image
```

This intelligent platform detection ensures your images support the same platforms as your base image, maintaining consistency throughout your image stack.

## Build Process

krust builds your Rust application in an isolated environment:

1. **Temporary build directory** - Each build uses a unique temporary directory via `--target-dir`
2. **Static compilation** - Builds with `RUSTFLAGS="-C target-feature=+crt-static"` for musl targets
3. **Cross-compilation** - Automatically configures the appropriate linker for the target platform
4. **Binary extraction** - Copies the built binary from the temp directory for packaging
5. **Container creation** - Packages the binary into a minimal OCI image

This approach ensures:
- No conflicts between concurrent builds
- Clean builds without interference from previous compilations
- Safe parallel execution of multiple krust instances

## Static Binaries

krust builds fully static binaries by default using:
- musl libc for Linux targets
- `RUSTFLAGS="-C target-feature=+crt-static"` for static linking
- Distroless static base image (`gcr.io/distroless/static:nonroot`)

This ensures your applications work across all Linux distributions without dependency issues.

### Why musl instead of glibc?

krust uses musl libc instead of glibc for several important reasons:

1. **True static linking** - musl is designed for static linking, while glibc uses dynamic loading internally (NSS) that breaks in static binaries
2. **Smaller binaries** - musl static binaries are typically 5-10x smaller than glibc equivalents
3. **No runtime surprises** - glibc static binaries often fail at runtime with DNS resolution, user lookups, or locale issues
4. **Container-optimized** - musl's simplicity makes it ideal for containers where you want minimal dependencies
5. **Security** - Smaller attack surface with fewer moving parts

The tradeoff is that musl has slightly different behavior than glibc in some edge cases, but for most applications this is not an issue. If your application requires glibc-specific behavior, you can override the default by building locally with cargo and creating your own container image.

## Environment Variables

- `KRUST_REPO` - Default repository prefix for built images

## Configuration

### Project Configuration (Cargo.toml)

You can configure krust on a per-project basis by adding a `[package.metadata.krust]` section to your project's `Cargo.toml`:

```toml
[package.metadata.krust]
base-image = "cgr.dev/chainguard/static:latest"  # Override the default base image
```

This is the idiomatic way to configure build tools in Rust, similar to how `cargo-deb`, `wasm-pack`, and other Cargo extensions work.

### Global Configuration

krust also looks for global configuration at `~/.config/krust/config.toml`:

```toml
base_image = "cgr.dev/chainguard/static:latest"  # Default base image for all projects
default_registry = "ghcr.io"

[build]
cargo_args = ["--features", "production"]
```

### Configuration Precedence

When determining the base image, krust uses this precedence order:
1. Project-specific config in `Cargo.toml` (highest priority)
2. Global config in `~/.config/krust/config.toml`
3. Built-in default: `cgr.dev/chainguard/static:latest` (lowest priority)

## Registry Authentication

krust automatically handles registry authentication using Docker's standard credential mechanisms:

### Docker Config Files

krust reads authentication from standard Docker config locations:
- `$DOCKER_CONFIG/config.json` (if DOCKER_CONFIG is set)
- `$REGISTRY_AUTH_FILE` (if set, takes precedence)
- `~/.docker/config.json` (default location)

### Docker Credential Helpers

krust supports Docker credential helpers for secure credential storage:
- Reads from `credHelpers` configuration for specific registries
- Falls back to `credsStore` for the default credential store
- Supports all standard Docker credential helpers (e.g., `docker-credential-desktop`, `docker-credential-pass`)

### Authentication Methods

krust automatically detects and uses the appropriate authentication:
- **Anonymous** - For public registries and images
- **Basic Auth** - Username and password authentication
- **Bearer Token** - OAuth2/JWT token authentication (e.g., for GitHub Container Registry)

Example Docker config with various auth methods:
```json
{
  "auths": {
    "docker.io": {
      "auth": "base64(username:password)"
    },
    "ghcr.io": {
      "registrytoken": "ghp_your_github_token"
    }
  },
  "credHelpers": {
    "gcr.io": "gcloud",
    "123456789.dkr.ecr.us-east-1.amazonaws.com": "ecr-login"
  },
  "credsStore": "desktop"
}
```

## Key Features

- **Docker-free** - Builds OCI container images without requiring Docker daemon
- **Static binaries** - Produces truly static binaries using musl libc
- **Composable** - Outputs image digest to stdout, enabling `docker run $(krust build)`
- **Multi-arch support** - Build for multiple platforms in a single command
- **Cross-platform** - Supports multiple architectures (amd64, arm64, arm/v7)
- **Minimal images** - Uses distroless base images for security and size
- **OCI compliant** - Works with any OCI-compliant container registry
- **Isolated builds** - Each build uses a temporary directory to avoid conflicts
- **Concurrent builds** - Multiple builds can run safely in parallel
- **Automatic authentication** - Seamlessly integrates with Docker credential helpers and config files

## Example

Build and run the example application:

```bash
# Set your repository
export KRUST_REPO=<repository-to-push-to>

# Build and push the example (default behavior)
krust build example/hello-krust

# Build without pushing
krust build example/hello-krust --no-push

# Build, push, and run the example
docker run --rm $(krust build example/hello-krust)

# Specify a tag to apply to the image
krust build example/hello-krust --tag v1.2.3
```

## CLI Reference

### Build Command

```
krust build [OPTIONS] [DIRECTORY] [-- <CARGO_ARGS>...]

Arguments:
  [DIRECTORY]      Path to the Rust project directory (defaults to current directory)
  [CARGO_ARGS]...  Additional cargo build arguments

Options:
  -i, --image <IMAGE>        Target image reference (overrides KRUST_REPO)
      --platform <PLATFORM>  Target platform [default: linux/amd64]
      --no-push              Skip pushing the image to registry
      --tag <TAG>            Tag to apply to the image (e.g., latest, v1.0.0)
      --repo <REPO>          Repository prefix (uses KRUST_REPO env var)
  -v, --verbose              Enable verbose logging
  -h, --help                 Print help
```

### Resolve Command

The `resolve` command scans YAML files for `krust://` references, builds the referenced images, and outputs resolved YAML with concrete image digests.

```
krust resolve -f <FILE_OR_DIR> [OPTIONS]

Arguments:
  -f, --filename <PATH>      Path to YAML file or directory (can be repeated)

Options:
      --platform <PLATFORM>  Target platforms (comma-separated)
      --repo <REPO>          Repository prefix (uses KRUST_REPO env var)
      --tag <TAG>            Tag to apply to built images
  -v, --verbose              Enable verbose logging
  -h, --help                 Print help
```

#### Usage Examples

```bash
# Resolve a single file
export KRUST_REPO=<repository-to-push-to>
krust resolve -f deployment.yaml > resolved.yaml

# Resolve multiple files
krust resolve -f deployment.yaml -f service.yaml

# Resolve all YAML files in a directory
krust resolve -f ./k8s/

# Pipe directly to kubectl
krust resolve -f deployment.yaml | kubectl apply -f -

# Build for multiple platforms
krust resolve -f deployment.yaml --platform linux/amd64,linux/arm64
```

#### YAML Reference Syntax

Use `krust://` prefix followed by the path to the Rust project:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
spec:
  template:
    spec:
      containers:
      - name: app
        image: krust://./path/to/rust/project
```

The `resolve` command will:
1. Find all `krust://` references (deduplicates automatically)
2. Build each unique project once
3. Push images to the registry
4. Replace references with concrete digests (i.e., `@sha256:...`)
5. Output resolved YAML to stdout

**Note**: Multiple references to the same path are deduplicated - the image is built only once and all references are updated with the same digest.

### Apply Command

The `apply` command combines `resolve` with `kubectl apply` for a seamless deployment workflow:

```
krust apply -f <FILE_OR_DIR> [OPTIONS]

Arguments:
  -f, --filename <PATH>      Path to YAML file or directory (can be repeated)

Options:
      --platform <PLATFORM>  Target platforms (comma-separated)
      --repo <REPO>          Repository prefix (uses KRUST_REPO env var)
      --tag <TAG>            Tag to apply to built images
  -v, --verbose              Enable verbose logging
  -h, --help                 Print help
```

#### Usage Examples

```bash
# Build and deploy in one command
export KRUST_REPO=<repository-to-push-to>
krust apply -f deployment.yaml

# Apply entire directory
krust apply -f ./k8s/

# Build for multiple platforms and deploy
krust apply -f deployment.yaml --platform linux/amd64,linux/arm64
```

The `apply` command is equivalent to:
```bash
krust resolve -f deployment.yaml | kubectl apply -f -
```

## Troubleshooting

### macOS: "linking with `cc` failed"

This error occurs when the cross-compilation toolchain is not properly configured. Make sure you:

1. Install musl-cross: `brew install filosottile/musl-cross/musl-cross`
2. Create `.cargo/config.toml` in your project with the appropriate linker configuration

### "target may not be installed"

Install the required target with rustup:
```bash
rustup target add x86_64-unknown-linux-musl
```

### Platform mismatch warning when running images

This is normal when building linux/amd64 images on Apple Silicon. The images will still run correctly under emulation.

## Development

### Setting up development environment

```bash
# Clone the repository
git clone https://github.com/imjasonh/krust.git
cd krust

# Install cross-compilation toolchain (required for tests)
# On macOS:
brew install messense/macos-cross-toolchains/x86_64-unknown-linux-musl
brew install messense/macos-cross-toolchains/aarch64-unknown-linux-musl

# For full platform support, consider using cargo-zigbuild:
cargo install cargo-zigbuild

# Install Rust targets (at minimum for tests)
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl

# For full platform support, add all targets:
rustup target add \
    armv7-unknown-linux-musleabihf \
    arm-unknown-linux-musleabihf \
    i686-unknown-linux-musl \
    powerpc64le-unknown-linux-musl \
    s390x-unknown-linux-musl \
    riscv64gc-unknown-linux-musl

# Install pre-commit hooks
pip install pre-commit
  or
brew install pre-commit

pre-commit install

# Build and test
make build
make test
```

### Makefile Targets

The project includes a comprehensive Makefile for common development tasks:

```bash
# Building
make build              # Build the project
make build-verbose      # Build with verbose output

# Testing (runs single-threaded to avoid env var races)
make test              # Run all tests
make test-unit         # Run unit tests only
make test-e2e          # Run end-to-end tests only
make test-verbose      # Run all tests with verbose output

# Code quality
make fmt               # Format code
make lint              # Run clippy linter
make check-fmt         # Check formatting without fixing
make check             # Run all checks (format, lint, test)

# Cross-compilation
make setup-cross-compile    # Set up cargo config for cross-compilation
make verify-cross-compile   # Verify cross-compilation setup
```

### Pre-commit hooks

This project uses pre-commit hooks to ensure code quality. The hooks will automatically:
- Check code formatting with `cargo fmt`
- Run `cargo clippy` to check for common mistakes
- Run `cargo check` to ensure the project compiles
- Run tests (single-threaded to avoid environment variable conflicts)
- Fix trailing whitespace and ensure files end with newline
- Validate YAML files

All hooks use the Makefile targets for consistency with CI.

To run the hooks manually:
```bash
pre-commit run --all-files
```

### Running tests

Tests are configured to run single-threaded by default to avoid race conditions with environment variable modifications (the credential helper tests modify DOCKER_CONFIG, HOME, etc.):

```bash
# Run all tests (using Makefile)
make test

# Run specific test suites
make test-unit         # Unit tests only
make test-e2e          # End-to-end tests only

# Run with cargo directly (remember --test-threads=1)
cargo test -- --test-threads=1

# Run with verbose output
make test-verbose
```

## License

MIT OR Apache-2.0
