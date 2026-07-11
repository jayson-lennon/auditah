# Maintainer: Jayson Lennon <jayson@jaysonlennon.dev>
pkgname=auditah
pkgver=0.1.1
pkgrel=1
pkgdesc="Obligation-aware license compliance + attribution tool for gamedev"
arch=('x86_64')
url="https://github.com/jayson-lennon/auditah"
license=('AGPL3')
makedepends=('cargo' 'git')
source=("auditah::git+$url.git#tag=v$pkgver")
sha256sums=('SKIP')
options=(!debug)

# Release-style profile: LTO + 1 codegen-unit for a smaller, optimized binary.
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
export CARGO_PROFILE_RELEASE_LTO=fat

prepare() {
    cd "$srcdir/$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    cd "$srcdir/$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    cargo build --release --frozen --offline
}

check() {
    cd "$srcdir/$pkgname"
    export RUSTUP_TOOLCHAIN=stable

    # Run the test suite (use cargo test if nextest isn't installed).
    if cargo nextest --version >/dev/null 2>&1; then
        cargo nextest run --release --frozen --offline
    else
        cargo test --release --frozen --offline
    fi
}

package() {
    cd "$srcdir/$pkgname"

    # Binary
    install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"

    # License: AGPL-3.0-or-later. The repo vendors the canonical SPDX text at
    # well_known_licenses/AGPL-3.0-or-later.txt; install it to satisfy AGPL §13
    # (license text must accompany distribution).
    install -Dm644 "well_known_licenses/AGPL-3.0-or-later.txt" \
        "$pkgdir/usr/share/licenses/$pkgname/AGPL-3.0-or-later.txt"
}
