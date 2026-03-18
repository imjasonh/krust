.PHONY: test test-unit test-e2e test-testscript build run clean fmt lint check-fmt check install

# Test flags - run single-threaded to avoid env var races
TEST_FLAGS := -- --test-threads=1

# Build the project
build:
	cargo build --verbose

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
	KRUST_REPO=ttl.sh/krust cargo run build ./example/hello-krust

run-built-image:
	@image=$$(KRUST_REPO=ttl.sh/krust cargo run build ./example/hello-krust) && \
	echo "Running image: $$image" && \
	docker run --rm $$image
