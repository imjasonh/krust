# krust Development Notes

This document captures key learnings and decisions made during the development of krust with Claude.

## Project Overview

krust is a container image build tool for Rust applications, inspired by ko.build for Go. It builds static binaries and packages them into minimal OCI container images without requiring Docker.

## Key Design Decisions

### 1. Static Binaries with musl

We chose musl libc over glibc for static linking because:
- **True static linking**: glibc uses dynamic loading internally (NSS) which breaks in static binaries
- **Smaller binaries**: musl static binaries are 5-10x smaller than glibc
- **No runtime surprises**: glibc static binaries often fail with DNS resolution, user lookups, or locale issues
- **Container-optimized**: Perfect for minimal container images

### 2. Default Push Behavior

krust pushes images by default (use `--no-push` to skip) because:
- Aligns with the common workflow of building and immediately using images
- Enables the `docker run $(krust build)` pattern
- Reduces friction for the most common use case

### 3. Output Design

- **stdout**: Only the pushed image reference by digest (e.g., `ttl.sh/user/app@sha256:...`)
- **stderr**: All logging and progress information
- This enables composability with other tools

### 4. Image Naming Strategy

- Uses `KRUST_REPO` environment variable for repository prefix
- Automatically appends project name from Cargo.toml
- Can be overridden with `--image` flag
- Default tag is `latest`

## Technical Learnings

### OCI Image Building

1. **Layer Digest vs Diff ID**:
   - Layer digest: SHA256 of the compressed (gzip) layer
   - Diff ID: SHA256 of the uncompressed tar (goes in image config)
   - Docker validates these match during pull

2. **Image Structure**:
   ```
   Manifest -> Config + Layers
   Config contains: architecture, OS, environment, command, diff_ids
   Layers contain: compressed tar.gz files
   ```

3. **Registry API**:
   - Push blobs (config and layers) first
   - Then push manifest referencing those blobs
   - Manifest URL contains the final digest

### Cross-Compilation on macOS

For Linux targets from macOS, you need:
1. Target toolchain: `rustup target add x86_64-unknown-linux-musl`
2. Cross-linker: `brew install filosottile/musl-cross/musl-cross`
3. Cargo config to specify the linker:
   ```toml
   [target.x86_64-unknown-linux-musl]
   linker = "x86_64-linux-musl-gcc"
   ```

### Rust Static Linking

- Use `RUSTFLAGS="-C target-feature=+crt-static"` for static linking
- musl targets default to static, but explicit is better
- The resulting binary has no runtime dependencies

## Architecture Decisions

### Module Structure

```
src/
├── main.rs          # CLI entry point and orchestration
├── lib.rs           # Public API exports
├── cli/             # Command-line interface definitions
├── builder/         # Rust compilation logic
├── image/           # OCI image construction
├── registry/        # Registry push operations
└── config/          # Configuration management
```

### Error Handling

- Used `anyhow` for error propagation with context
- Errors include contextual information for debugging
- All errors go to stderr, preserving stdout for output

### Dependencies

Key crates chosen:
- `clap` - CLI parsing with derive macros
- `tokio` - Async runtime for registry operations
- `oci-distribution` - OCI registry client
- `tar` + `flate2` - Layer creation
- `sha256` - Digest calculation
- `tracing` - Structured logging

## Testing Strategy

1. **Unit tests** for each module
2. **Integration tests** for CLI commands
3. **E2E tests** that actually run the built binary
4. Used `assert_cmd` for testing CLI behavior

## Development Workflow

The iterative development process:
1. Start with basic CLI structure
2. Implement core functionality (build, image, push)
3. Test with real registries (ttl.sh for anonymous push)
4. Fix issues discovered during real usage
5. Refine UX based on actual workflows

## Future Improvements

Potential enhancements identified:
1. Registry authentication support
2. Multi-platform image manifests
3. Build caching
4. Image layer optimization
5. Support for custom Dockerfile-like configs
6. SBOM (Software Bill of Materials) generation

## Useful Commands

```bash
# Test the full workflow
export KRUST_REPO=ttl.sh/test
docker run $(krust build example/hello-krust)

# Debug output
krust build -v 2>&1 | less

# Check static linking
file target/x86_64-unknown-linux-musl/release/binary
ldd target/x86_64-unknown-linux-musl/release/binary  # should say "not a dynamic executable"
```

## Resources

- [OCI Image Spec](https://github.com/opencontainers/image-spec)
- [OCI Distribution Spec](https://github.com/opencontainers/distribution-spec)
- [ko.build](https://ko.build) - Inspiration for this project
- [ttl.sh](https://ttl.sh) - Anonymous ephemeral registry for testing