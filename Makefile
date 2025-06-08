.PHONY: test test-unit test-e2e build run clean fmt lint check-fmt check test-verbose setup-cross-compile verify-cross-compile

# Test flags - run single-threaded to avoid env var races
TEST_FLAGS := -- --test-threads=1

# Build the project
build:
	cargo build

# Build verbosely
build-verbose:
	cargo build --verbose

# Setup cargo config for cross-compilation
setup-cross-compile:
	@mkdir -p .cargo
	@cat > .cargo/config.toml <<'EOF'
	[target.x86_64-unknown-linux-musl]
	linker = "x86_64-linux-musl-gcc"

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

# Run all tests verbosely (for CI)
test-verbose:
	cargo test --verbose $(TEST_FLAGS)

# Run unit tests only
test-unit:
	cargo test --lib --bins $(TEST_FLAGS)

# Run e2e tests only
test-e2e:
	cargo test --test '*' $(TEST_FLAGS)

# Run e2e tests verbosely (for CI)
test-e2e-verbose:
	cargo test --test '*' --verbose $(TEST_FLAGS)

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
