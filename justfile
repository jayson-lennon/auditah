# auditah justfile
# Reference: https://github.com/casey/just

# Default target: run tests
default: test

# Build the binary
build:
    cargo build

# Run all tests
test:
    cargo test

# Check without producing artifacts
check:
    cargo check

# Format check
fmt-check:
    cargo fmt -- --check

# Apply formatting
fmt-fix:
    cargo fmt

# Run the license compliance audit against the current directory.
audit:
    cargo run -- audit

# Generate CREDITS.md from attribution sidecars/manifests.
credits:
    cargo run -- credits

# Alias: license compliance lint.
lint: audit

# Run clippy lints.
clippy:
    cargo clippy --all-targets -- -D warnings
