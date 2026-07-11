# auditah justfile
# Reference: https://github.com/casey/just

# Run all tests
test:
    cargo nextest run

# Build the binary
build:
    cargo build

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

# Build the Arch package in ./build (isolated from the source tree).
# Creates build/auditah-git-*.pkg.tar.zst. build/ is gitignored.
pkg:
    @mkdir -p build
    @cp PKGBUILD build/
    @cd build && makepkg -f
