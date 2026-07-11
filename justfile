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

# Generate all distribution artifacts (CREDITS.md, NOTICES.md, BOM.md).
generate:
    cargo run -- generate

# Alias: license compliance lint.
lint: audit

# Run clippy lints.
clippy:
    cargo clippy --all-targets -- -D warnings

# Build the Arch package in ./build (isolated from the source tree).
# Creates build/auditah-*.pkg.tar.zst. build/ is gitignored.
pkg:
    @mkdir -p build
    @cp PKGBUILD build/
    @cd build && makepkg -f

# Bump version (major/minor/patch), commit, tag, and push.
# Aborts if the working tree has uncommitted changes.
bump LEVEL:
    #!/usr/bin/env bash
    set -euo pipefail

    # --- Validate input ---
    case "{{LEVEL}}" in
        major|minor|patch) ;;
        *) echo "Usage: just bump <major|minor|patch>" >&2; exit 1 ;;
    esac

    # --- Pre-flight: working tree must be clean ---
    if ! git diff-index --quiet HEAD --; then
        echo "Error: working tree has uncommitted changes" >&2
        exit 1
    fi

    # --- Compute new version ---
    CURRENT=$(grep -m1 '^version = "' Cargo.toml | sed -E 's/^version = "(.*)"/\1/')
    NEW=$(rust-script scripts/bump-version.rs "$CURRENT" "{{LEVEL}}")

    # --- Update files ---
    sed -i "0,/^version = /{s/^version = \".*\"/version = \"$NEW\"/}" Cargo.toml
    sed -i "s/^pkgver=.*/pkgver=$NEW/" PKGBUILD
    sed -i "s/^pkgrel=.*/pkgrel=1/" PKGBUILD

    # --- Regenerate Cargo.lock for the new version ---
    cargo update -p auditah --precise "$NEW"

    # --- Commit, tag, push ---
    git add Cargo.toml Cargo.lock PKGBUILD
    git commit -m "Bump version to ${NEW}"
    git tag "v${NEW}"
    git push
    git push --tags
