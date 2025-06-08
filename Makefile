.PHONY: test test-unit test-e2e build run clean fmt lint check-fmt check test-verbose

# Test flags - run single-threaded to avoid env var races
TEST_FLAGS := -- --test-threads=1

# Build the project
build:
	cargo build

# Build verbosely
build-verbose:
	cargo build --verbose

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
