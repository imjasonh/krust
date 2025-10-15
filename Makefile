.PHONY: test test-unit test-e2e test-testscript build run clean fmt lint check-fmt check install setup-cross-compile verify-cross-compile

# Test flags - run single-threaded to avoid env var races
TEST_FLAGS := -- --test-threads=1

# Build the project
build:
	cargo build --verbose

# Setup cargo config for cross-compilation
setup-cross-compile:
	@mkdir -p .cargo
	@cat > .cargo/config.toml <<'EOF'
	[target.x86_64-unknown-linux-musl]
	linker = "musl-gcc"

	[target.aarch64-unknown-linux-musl]
	linker = "aarch64-linux-gnu-gcc"
	EOF
	@echo "Created .cargo/config.toml for cross-compilation"

# Verify cross-compilation setup
verify-cross-compile:
	@echo "=== Rust Installation ==="
	@echo "Installed targets:"
	@rustup target list --installed
	@echo ""
	@echo "Cargo version:"
	@cargo --version
	@echo ""
	@echo "Rustc version:"
	@rustc --version
	@echo ""
	@echo "=== Available Linkers ==="
	@which x86_64-unknown-linux-musl-gcc 2>/dev/null && echo "✓ x86_64-unknown-linux-musl-gcc found" || echo "✗ x86_64-unknown-linux-musl-gcc not found"
	@which x86_64-linux-musl-gcc 2>/dev/null && echo "✓ x86_64-linux-musl-gcc found" || echo "✗ x86_64-linux-musl-gcc not found"
	@which musl-gcc 2>/dev/null && echo "✓ musl-gcc found" || echo "✗ musl-gcc not found"
	@which aarch64-linux-gnu-gcc 2>/dev/null && echo "✓ aarch64-linux-gnu-gcc found" || echo "✗ aarch64-linux-gnu-gcc not found"
	@which rust-lld 2>/dev/null && echo "✓ rust-lld found" || echo "✗ rust-lld not found"
	@echo ""
	@echo "=== Cargo Config ==="
	@if [ -f .cargo/config.toml ]; then \
		cat .cargo/config.toml; \
	else \
		echo "No cargo config found at .cargo/config.toml"; \
	fi

# Run the project
run:
	cargo run

# Run all tests
test: test-unit test-e2e

# Run unit tests only
test-unit:
	cargo test --verbose --lib --bins $(TEST_FLAGS)

# Run e2e tests only
test-e2e:
	cargo test --verbose --test '*' $(TEST_FLAGS)

# Run testscript tests only
test-testscript:
	cargo test --verbose --test testscript_test

# Install krust to ~/.local/bin
install:
	@echo "Building krust in release mode..."
	@cargo build --release
	@mkdir -p ~/.local/bin
	@cp target/release/krust ~/.local/bin/krust
	@echo "Installed krust to ~/.local/bin/krust"
	@echo "Make sure ~/.local/bin is in your PATH"

# Clean build artifacts
clean:
	cargo clean

# Format code
fmt:
	cargo fmt

# Run linter
lint:
	cargo clippy -- -D warnings

# Check formatting
check-fmt:
	cargo fmt -- --check

# Check code (for pre-commit)
check-code:
	cargo check

# Run all checks (format, lint, test)
check: check-fmt lint test

push-ttl:
	@echo "Pushing to ttl.sh..."
	KRUST_REPO=ttl.sh/jason cargo run build ./example/hello-krust

push-gar:
	@echo "Pushing to gar.sh..."
	KRUST_REPO=us-central1-docker.pkg.dev/jason-chainguard/krust cargo run build ./example/hello-krust

run-built-image:
	@image=$$(KRUST_REPO=ttl.sh/jason cargo run build ./example/hello-krust) && \
	echo "Running image: $$image" && \
	docker run --rm $$image
