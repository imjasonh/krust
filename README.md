# krust

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
export KRUST_REPO=ttl.sh/$USER

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

# For linux/arm/v7
rustup target add armv7-unknown-linux-musleabihf
```

#### macOS Cross-compilation Setup

On macOS, you'll need a cross-compilation toolchain:

```bash
# Install musl cross-compilation tools
brew install filosottile/musl-cross/musl-cross

# Create a .cargo/config.toml in your project with:
cat > .cargo/config.toml << 'EOF'
[target.x86_64-unknown-linux-musl]
linker = "x86_64-linux-musl-gcc"

[target.aarch64-unknown-linux-musl]
linker = "aarch64-linux-musl-gcc"
EOF
```

Note: krust builds fully static binaries by default using musl libc, ensuring maximum portability across different Linux distributions and container environments.

## Usage

krust outputs the pushed image reference by digest to stdout, with all other output going to stderr. This enables composability with other tools.

### Build a project in the current directory

```bash
# Set your repository prefix
export KRUST_REPO=ttl.sh/jason

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

# Build for a different platform
krust build --platform linux/arm64
```

### Build with custom cargo arguments

```bash
krust build -- --features=prod
```

## Supported Platforms

- `linux/amd64` (x86_64-unknown-linux-musl)
- `linux/arm64` (aarch64-unknown-linux-musl)
- `linux/arm/v7` (armv7-unknown-linux-musleabihf)

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

- `KRUST_REPO` - Default repository prefix for built images (e.g., `ttl.sh/username`)
- `KRUST_IMAGE` - Override the full image reference for a build

## Configuration

krust looks for configuration at `~/.config/krust/config.toml`:

```toml
base_image = "gcr.io/distroless/static:nonroot"
default_registry = "ghcr.io"

[build]
cargo_args = ["--features", "production"]

[registries."ghcr.io"]
username = "myuser"
password = "mytoken"
```

Note: Registry authentication is not yet implemented. Currently, krust uses anonymous authentication.

## Key Features

- **Docker-free** - Builds OCI container images without requiring Docker daemon
- **Static binaries** - Produces truly static binaries using musl libc
- **Composable** - Outputs image digest to stdout, enabling `docker run $(krust build)`
- **Cross-platform** - Supports multiple architectures (amd64, arm64, arm/v7)
- **Minimal images** - Uses distroless base images for security and size
- **OCI compliant** - Works with any OCI-compliant container registry

## Example

Build and run the example application:

```bash
# Set your repository (ttl.sh provides temporary anonymous storage)
export KRUST_REPO=ttl.sh/jason

# Build and push the example (default behavior)
krust build example/hello-krust

# Build without pushing
krust build example/hello-krust --no-push

# Build, push, and run the example
docker run $(krust build example/hello-krust)

# Or specify a custom image name with TTL (time-to-live)
# Images on ttl.sh expire based on the tag: 1h, 2d, 1w, etc.
krust build example/hello-krust --image ttl.sh/jason/hello:1h
```

Note: [ttl.sh](https://ttl.sh) is a free, temporary container registry perfect for testing. Images are automatically deleted after their TTL expires.

## CLI Reference

```
krust build [OPTIONS] [DIRECTORY] [-- <CARGO_ARGS>...]

Arguments:
  [DIRECTORY]      Path to the Rust project directory (defaults to current directory)
  [CARGO_ARGS]...  Additional cargo build arguments

Options:
  -i, --image <IMAGE>        Target image reference (overrides KRUST_REPO)
      --platform <PLATFORM>  Target platform [default: linux/amd64]
      --no-push              Skip pushing the image to registry
      --repo <REPO>          Repository prefix (uses KRUST_REPO env var)
  -v, --verbose              Enable verbose logging
  -h, --help                 Print help
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

## License

MIT OR Apache-2.0
