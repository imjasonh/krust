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

### Google Artifact Registry (GAR) Blob Uploads

GAR has special handling for blob uploads that differs from the standard OCI spec:

1. **Location Header Format**:
   - POST to `/v2/.../blobs/uploads/` returns `location: /artifacts-uploads/...`
   - The location is a relative path starting with `/artifacts-uploads/`, NOT `/v2/`
   - Must build absolute URL as `https://{registry}{location}` for ANY path starting with `/`

2. **Upload Flow** (Resumable):
   - **Monolithic upload not supported**: PUT with body to `location?digest=` returns `301 Moved Permanently`
   - Must use resumable upload flow instead:
     1. POST to `/v2/.../blobs/uploads/` → get upload location
     2. PATCH to upload location with blob data → returns `202 Accepted`
     3. PUT to finalize location with `?digest=` and empty body → returns `201 Created`

3. **Critical Implementation Details**:
   - GAR returns `301` redirect on monolithic PUT attempts (not `307`)
   - HTTP spec says `301` means don't resend body, so automatic redirect following fails
   - Must explicitly handle `301` as a signal to switch to resumable upload
   - PATCH response may include a new `location` header for the finalize PUT
   - Use `reqwest` with `redirect::Policy::none()` to handle redirects manually

4. **Why reqwest over hyper**:
   - Initially used raw `hyper` but it doesn't auto-follow redirects with request bodies
   - Switched to `reqwest` for cleaner API and better redirect handling
   - Disabled automatic redirects to manually handle GAR's upload flow

5. **Memory Efficiency**:
   - Blob downloads return `bytes::Bytes` instead of `Vec<u8>` to avoid unnecessary copies
   - Blob uploads require `.to_vec()` due to reqwest's `'static` requirement for request bodies
   - This is acceptable as reqwest streams the data internally

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
- `reqwest` - HTTP client with automatic redirect handling
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

## Features Implemented

### YAML Resolution (`krust resolve`)

Inspired by ko's Kubernetes integration, krust can resolve `krust://` references in YAML files:

1. **Reference Syntax**: Use `krust://path/to/project` in YAML (e.g., `image: krust://./example/hello-krust`)
2. **Deduplication**: Multiple references to the same path are deduplicated - builds only once
3. **Multi-document support**: Handles YAML files with multiple `---` separated documents
4. **Directory support**: Can process entire directories of YAML files with `-f ./k8s/`
5. **Output**: Resolved YAML to stdout with all `krust://` replaced by digests

**Implementation details**:
- Uses `serde_yml` (maintained fork of deprecated serde_yaml) for parsing and serialization
- Recursively walks YAML tree to find all string values with `krust://` prefix
- Builds each unique path once, stores digest mapping
- Second pass replaces all references with digests
- Preserves YAML structure and formatting

**Usage**:
- `krust resolve -f deployment.yaml | kubectl apply -f -`
- Or use the convenience command: `krust apply -f deployment.yaml`

### Apply Command (`krust apply`)

Convenience wrapper that combines `resolve` with `kubectl apply`:
- Resolves `krust://` references
- Pipes resolved YAML directly to `kubectl apply -f -`
- Exits with kubectl's exit code
- Requires `kubectl` to be installed and configured

**Usage**: `krust apply -f deployment.yaml`

## Future Improvements

Potential enhancements identified:
1. ~~Registry authentication support~~ ✓ Implemented (supports Docker credential helpers)
2. ~~YAML resolution for Kubernetes deployments~~ ✓ Implemented (`krust resolve`)
3. Multi-platform image manifests
4. Build caching
5. Image layer optimization
6. Support for custom Dockerfile-like configs
7. SBOM (Software Bill of Materials) generation
8. ~~Optimize blob uploads (check if blob exists before uploading)~~ ✓ Implemented
9. Stream uploads from disk instead of buffering in memory
   - Currently buffers tar and compressed layer in memory
   - Could write to temp file, calculate diff_id, then stream upload
   - Would reduce memory usage for large binaries

## Useful Commands

```bash
# Test with anonymous registry (ttl.sh)
export KRUST_REPO=ttl.sh/test
docker run $(krust build example/hello-krust)

# Test with Google Artifact Registry
export KRUST_REPO=us-central1-docker.pkg.dev/project-id/repo-name
krust build example/hello-krust

# Debug output
krust build -v 2>&1 | less

# Check static linking
file target/x86_64-unknown-linux-musl/release/binary
ldd target/x86_64-unknown-linux-musl/release/binary  # should say "not a dynamic executable"

# Verify pushed image
crane manifest $(krust build --no-push example/hello-krust)
```

## Resources

- [OCI Image Spec](https://github.com/opencontainers/image-spec)
- [OCI Distribution Spec](https://github.com/opencontainers/distribution-spec)
- [ko.build](https://ko.build) - Inspiration for this project
- [ttl.sh](https://ttl.sh) - Anonymous ephemeral registry for testing
