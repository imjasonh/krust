.PHONY: test test-unit test-e2e build run clean fmt lint

# Build the project
build:
	cargo build

# Run the project
run:
	cargo run

# Run all tests
test: test-unit test-e2e

# Run unit tests only
test-unit:
	cargo test --lib --bins

# Run e2e tests only
test-e2e:
	cargo test --test '*'

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

# Run all checks (format, lint, test)
check: check-fmt lint test