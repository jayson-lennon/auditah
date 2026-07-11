# Maintainer: Jayson Lennon <jayson@jaysonlennon.dev>
pkgname=auditah-git
pkgver=0.1.0.r0.g48145da
pkgrel=1
pkgdesc="Obligation-aware license compliance + attribution tool for gamedev"
arch=('x86_64')
url="https://github.com/jayson-lennon/auditah"
license=('AGPL3')
makedepends=('cargo' 'git')
provides=("${pkgname%-git}=$pkgver")
conflicts=("${pkgname%-git}")
source=("${pkgname%-git}::git+$url.git")
sha256sums=('SKIP')
options=(!debug)  # release binary has no debug symbols to split

# Release-style profile: LTO + 1 codegen-unit for a smaller, optimized binary.
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
export CARGO_PROFILE_RELEASE_LTO=fat

pkgver() {
    cd "$srcdir/${pkgname%-git}"

    # No release tags exist yet. Fall back to a stable, sortable pkgver
    # derived from the Cargo.toml version + git describe over all commits.
    # Format: <cargo_ver>.r<commits>.g<short_sha>
    local cargo_ver
    cargo_ver="$(grep -m1 '^version' Cargo.toml | sed -E 's/^version *= *"(.*)".*/\1/')"
    printf '%s.r%s.g%s' \
        "$cargo_ver" \
        "$(git rev-list --count HEAD)" \
        "$(git rev-parse --short HEAD)"
}

prepare() {
    cd "$srcdir/${pkgname%-git}"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    cd "$srcdir/${pkgname%-git}"
    export RUSTUP_TOOLCHAIN=stable

    # --offline because cargo fetch already populated the registry in prepare()
    cargo build --release --frozen --offline
}

check() {
    cd "$srcdir/${pkgname%-git}"
    export RUSTUP_TOOLCHAIN=stable

    # Run the test suite (use cargo test if nextest isn't installed).
    if cargo nextest --version >/dev/null 2>&1; then
        cargo nextest run --release --frozen --offline
    else
        cargo test --release --frozen --offline
    fi
}

package() {
    cd "$srcdir/${pkgname%-git}"

    # Binary
    install -Dm755 "target/release/${pkgname%-git}" \
        "$pkgdir/usr/bin/${pkgname%-git}"

    # License: AGPL-3.0-or-later. The repo vendors the canonical SPDX text at
    # well_known_licenses/AGPL-3.0-or-later.txt; install it to satisfy AGPL §13
    # (license text must accompany distribution).
    install -Dm644 "well_known_licenses/AGPL-3.0-or-later.txt" \
        "$pkgdir/usr/share/licenses/$pkgname/AGPL-3.0-or-later.txt"
}
