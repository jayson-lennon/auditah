# Semver Releases + `just bump`

## Problem

auditah's package uses a `-git` `pkgver()` that derives versions from git describe (`0.1.0.r36.g48145da`). The user wants clean semver release versions matching jinn's workflow (`auditah-0.1.1-1`, no hashes). There's no version-bump automation; cuts are manual and error-prone.

`--version` already exists (`#[command(version)]` on the `Cli` struct in `src/main.rs:18` → clap reads `CARGO_PKG_VERSION` → `auditah 0.1.0`). No flag work needed.

## Solution

1. **Port `scripts/bump-version.rs`** verbatim from jinn (`rust-script` + `semver` crate; parse → increment → print).
2. **Add `just bump <field>`** (major|minor|patch): abort if working tree dirty → run `bump-version.rs` → sed-update `Cargo.toml` `[package] version` + PKGBUILD `pkgver` + reset `pkgrel=1` → commit → tag `v<version>` → **push commits + tags**.
3. **Rewrite PKGBUILD** from `-git`/remote-fetch-with-`pkgver()` to static `pkgver` + tag-pinned fetch: drop `-git` suffix and `pkgver()`; set `source=("auditah::git+$url.git#tag=v$pkgver")`. Output filename becomes `auditah-0.1.0-1-x86_64.pkg.tar.zst`.
4. **Tag `v0.1.0`** as the baseline release tag on current HEAD.

---

## Dialectical Outcomes (Why)

### D1: Static `pkgver` (not `pkgver()`) — to kill the git-hash suffix
The `-git` `pkgver()` model produces `0.1.0.r36.g48145da` — unambiguous but ugly, and not what the user wants in a release artifact. jinn's model uses a static `pkgver=X.Y.Z` that the bump recipe mutates directly via sed. Package filenames become clean (`auditah-0.1.1-1`). Rejected alternative: keep `pkgver()` but force it to print only the cargo version — rejected because it discards the disambiguation that `-git` is for, and the user explicitly wants the jinn model.

### D2: Remote-fetch packaging (kept), not local-checkout (jinn's model)
jinn uses `source=()` + a `prepare()` symlink of `$startdir` into `$srcdir` because jinn installs complex resources (themes/personas/plugins/skills) and the local-checkout model simplifies that. auditah has no such complexity — its PKGBUILD is already structured for remote fetch. Switching to local-checkout would rewrite a working build for no gain. Kept remote-fetch; pinned to tag.

### D3: `bump` auto-pushes commits + tags (chosen over local-only)
A `pkgver()`-free, tag-pinned remote-fetch model requires the tag to exist on the remote before `just pkg` runs (makepkg clones `#tag=v$pkgver`). If `bump` only tagged locally, `just pkg` would fail until a manual push. The user's stated workflow is `just bump <field>` → `just pkg` → done, with no intermediate step. Auto-push makes that work. Rejected: local-only tagging (breaks the workflow); push-as-side-effect-of-pkg (weird coupling).

### D4: `bump` aborts on dirty tree (pre-flight gate)
Prevents a release from being cut from a half-committed state. Uses `git diff-index --quiet HEAD --` (empty exit = clean). Matches jinn's pre-flight. Non-negotiable per the user.

### D5: Port `bump-version.rs` verbatim (not bash semver)
`rust-script` confirmed available (`v0.21.0`). The jinn script is ~40 lines, proven, uses the real `semver` crate (correct parse/increment, no regex fragility). Reimplementing in bash would be bug-prone. Verbatim port keeps the two projects consistent.

### D6: `--version` requires no work
`src/main.rs:18` already has `#[command(version)]` which clap wires to `env!("CARGO_PKG_VERSION")`. Running `auditah --version` returns `auditah 0.1.0`. The bump recipe mutates `Cargo.toml` `version`, which cargo propagates to `CARGO_PKG_VERSION` at build time. No code change.

### D7: Baseline `v0.1.0` tag at current HEAD
auditah has no tags. The first `just bump patch` produces `v0.1.1`, but tagging the current released state as `v0.1.0` first gives a clean baseline. (Alternative — skip the baseline and let the first bump create the first tag — rejected as it leaves 0.1.0 untagged, breaking the "every release has a tag" invariant.)

---

## Relevant Files (Where)

| File | Action |
|---|---|
| `scripts/bump-version.rs` | **Create.** Verbatim port from `/mnt/zed/repos/jinn/workspace/scripts/bump-version.rs`. |
| `justfile` | **Modify.** Add `bump LEVEL` recipe after the existing `pkg` recipe. |
| `PKGBUILD` | **Modify.** Rewrite: drop `-git` suffix, drop `pkgver()`, static `pkgver`, tag-pinned `source`. |
| `Cargo.toml` | **No code change** — bump recipe mutates `version` via sed at release time. |
| `src/main.rs:18` | **No change** — `#[command(version)]` already present. |

---

## Key Code Context (What)

### Existing: `src/main.rs:18` (already correct — no change)
```rust
/// Top-level CLI.
#[derive(Debug, Parser)]
#[command(name = "auditah", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
```
`version` here tells clap to add `--version` using `CARGO_PKG_VERSION`. Already works.

### Existing: `Cargo.toml` `[package]` (mutated by bump recipe, not edited now)
```toml
[package]
name = "auditah"
version = "0.1.0"
edition = "2021"
description = "Obligation-aware license compliance + attribution tool for gamedev"
license = "AGPL-3.0-or-later"
```
The bump recipe does: `sed -i "s/^version = \".*\"/version = \"$NEW\"/" Cargo.toml` — but ONLY inside the `[package]` table. Because auditah is a single-crate (no `[workspace.package]`), the sed can be unscoped (only one `version =` line at package level). Note: jinn's sed is scoped to `[workspace.package]` because jinn is a workspace. For auditah, an unscoped `sed -i "0/^version = /s//version = \"$NEW\"/"` (first match only) is safe.

### Source to port: `/mnt/zed/repos/jinn/workspace/scripts/bump-version.rs`
```rust
//! ```cargo
//! [dependencies]
//! semver = "1"
//! ```

use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: bump-version <version> <major|minor|patch>");
        exit(1);
    }

    let version_str = &args[1];
    let level = &args[2];

    let mut version = match version_str.parse::<semver::Version>() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Invalid version '{version_str}': {e}");
            exit(1);
        }
    };

    match level.as_str() {
        "major" => {
            version.major += 1;
            version.minor = 0;
            version.patch = 0;
        }
        "minor" => {
            version.minor += 1;
            version.patch = 0;
        }
        "patch" => {
            version.patch += 1;
        }
        _ => {
            eprintln!("Invalid bump level '{level}'. Expected major, minor, or patch.");
            exit(1);
        }
    }

    print!("{version}");
}
```

### Existing: current `justfile` `pkg` recipe (unchanged)
```just
pkg:
    @mkdir -p build
    @cp PKGBUILD build/
    @cd build && makepkg -f
```

### Existing: current PKGBUILD (to be rewritten)
The full current PKGBUILD is reproduced in the Phases section (Phase 3) with strike-through annotations showing what changes.

---

## Implementation Algorithm (How)

### Phase 1: `scripts/bump-version.rs`
1. Create `scripts/` directory.
2. Write `scripts/bump-version.rs` verbatim from jinn (reproduced in Key Code Context).
3. Verify: `rust-script scripts/bump-version.rs 0.1.0 patch` → prints `0.1.1`.

### Phase 2: `just bump LEVEL` recipe
The recipe is a single bash script (shebang line) with these steps, in order:

```just
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
    sed -i "0/^version = /{s/^version = \".*\"/version = \"$NEW\"/}" Cargo.toml
    sed -i "s/^pkgver=.*/pkgver=$NEW/" PKGBUILD
    sed -i "s/^pkgrel=.*/pkgrel=1/" PKGBUILD

    # --- Commit, tag, push ---
    git add Cargo.toml PKGBUILD
    git commit -m "Bump version to ${NEW}"
    git tag "v${NEW}"
    git push
    git push --tags
```

Key correctness points:
- `grep -m1 '^version = "'` matches only lines starting with `version = "` (the `[package]` version), not dependencies' `version =` lines.
- `sed -i "0/^version = /{...}` operates on the first matching line only (belt-and-suspenders with the grep).
- `git diff-index --quiet HEAD --` exits 0 (clean) / non-zero (dirty). Note: untracked files are NOT detected by `diff-index`; if that matters, use `git status --porcelain` instead. For this project, `diff-index` matches jinn's behavior and is sufficient.
- `git add Cargo.toml PKGBUILD` stages only the two bumped files (the commit is scoped, not `git add -A`).

### Phase 3: PKGBUILD rewrite
The new PKGBUILD drops `-git` semantics entirely:

```bash
# Maintainer: Jayson Lennon <jayson@jaysonlennon.dev>
pkgname=auditah
pkgver=0.1.0
pkgrel=1
pkgdesc="Obligation-aware license compliance + attribution tool for gamedev"
arch=('x86_64')
url="https://github.com/jayson-lennon/auditah"
license=('AGPL3')
makedepends=('cargo' 'git')
source=("auditah::git+$url.git#tag=v$pkgver")
sha256sums=('SKIP')
options=(!debug)

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
    if cargo nextest --version >/dev/null 2>&1; then
        cargo nextest run --release --frozen --offline
    else
        cargo test --release --frozen --offline
    fi
}

package() {
    cd "$srcdir/$pkgname"
    install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
    install -Dm644 "well_known_licenses/AGPL-3.0-or-later.txt" \
        "$pkgdir/usr/share/licenses/$pkgname/AGPL-3.0-or-later.txt"
}
```

Changes from the old PKGBUILD:
- `pkgname=auditah-git` → `pkgname=auditah` (drop `-git` suffix)
- `pkgver=0.1.0.r0.g48145da` → `pkgver=0.1.0` (static, no git describe)
- Remove `provides=` / `conflicts=` (no longer a `-git` package conflicting with a release)
- Remove the entire `pkgver()` function
- `source=("${pkgname%-git}::git+$url.git")` → `source=("auditah::git+$url.git#tag=v$pkgver")` (pin to tag)
- All `${pkgname%-git}` references → `$pkgname` (since pkgname no longer has `-git`)

### Phase 4: Baseline `v0.1.0` tag
1. Verify current HEAD is the intended 0.1.0 state.
2. `git tag v0.1.0`
3. `git push origin v0.1.0` (or `git push --tags`)

### Phase 5: Verification
- Run `cargo build --tests`, `cargo clippy --tests`, `cargo fmt --check`, `cargo nextest run` — all must be clean.
- Dry-run the bump logic on a throwaway branch to confirm sed targets + push behavior without polluting main.
- Validate `just bump patch && just pkg` produces `build/auditah-0.1.1-1-x86_64.pkg.tar.zst`.
- Validate `auditah --version` reports the bumped version.

---

## Anti-Goals (Out of Scope)

1. **No `--version` flag implementation** — already exists via `#[command(version)]`.
2. **No local-checkout packaging rewrite** — kept remote-fetch (jinn's local-checkout is for its resource-installation complexity, which auditah lacks).
3. **No `[workspace.package]` refactor** — auditah is a single crate; the version lives in `[package]`.
4. **No changelog generation** — out of scope; the commit `Bump version to X` is the record.
5. **No pre-release / dev / alpha version support** — jinn's `bump-version.rs` only handles major/minor/patch on full semver; no `-alpha`/`-beta` suffixes. Don't extend it.
6. **No CI integration** — the workflow is local `just` commands, not GitHub Actions.
7. **No signing of packages or tags** — unsigned is fine for now.
8. **No `provides`/`conflicts` dance** — the package is no longer `-git`, so there's no release/conflict semantics to model.

---

## Edge Cases & Gotchas

1. **`git diff-index --quiet HEAD --` does NOT detect untracked files.** If the working tree has untracked files (e.g., a stray `foo.rs`), `diff-index` reports clean. For this project that's acceptable (matches jinn's behavior). If stricter checking is desired, swap to `git status --porcelain`.
2. **The sed for Cargo.toml must hit ONLY the `[package]` version line.** `grep -m1 '^version = "'` + `sed "0/^version = /...` (first-match) ensures it never touches a dependency's `version =` line. Do NOT use an unscoped `sed -i "s/^version.*/.../"` — that would clobber every dependency version in the file.
3. **Tag must be pushed before `just pkg` runs.** `bump` handles this (auto-push). If a user runs `just pkg` without having run `bump`, makepkg's `#tag=v$pkgver` clone will fetch whatever tag matches `pkgver` — if none exists (e.g., pre-baseline), the clone fails with a git error. The `v0.1.0` baseline tag (Phase 4) ensures 0.1.0 resolves.
4. **`rust-script` must be installed** (`cargo install rust-script`). Confirmed present at `~/.cargo/bin/rust-script` v0.21.0. If missing, `bump` fails at the `bump-version.rs` invocation.
5. **`makepkg` runs as non-root** — standard Arch constraint, already satisfied.
6. **`just pkg` still isolates to `./build/`.** The recipe is unchanged: copies PKGBUILD to `build/`, runs makepkg there. The BUILDDIR-vs-repo-root isolation (the auditah incident) is preserved.
7. **Push failure modes.** If `git push` fails (network, auth, non-fast-forward), `set -euo pipefail` aborts after the tag is created locally. Recovery: `git tag -d v$NEW` (delete local tag), fix the issue, re-run. The local tag is recoverable; the remote tag (if partially pushed) may need `git push origin :refs/tags/v$NEW` to delete.
8. **`pkgrel` reset to 1.** Every bump resets `pkgrel=1` (standard Arch practice — a new upstream version starts a fresh pkgrel counter).
9. **The `0/^version = /` sed address is GNU sed specific.** Arch Linux uses GNU sed, so this is fine. Don't port to BSD sed without adapting.

---

## Navigation Anchors

- **`src/main.rs:18`** — the `#[command(version)]` attribute. Confirms `--version` already works; no change.
- **`Cargo.toml` `[package]`** — the `version = "0.1.0"` line that the bump recipe mutates.
- **`PKGBUILD`** — rewritten in Phase 3. Drop `-git`, drop `pkgver()`, pin source to tag.
- **`justfile` `pkg` recipe** — unchanged; the isolation model is correct.
- **`scripts/bump-version.rs`** — new; the semver engine.
- **`/mnt/zed/repos/jinn/workspace/scripts/bump-version.rs`** — the reference source to port verbatim.
- **`/mnt/zed/repos/jinn/workspace` justfile `bump LEVEL` recipe** — the reference recipe structure (Fossil-specific parts adapted to git).

---

## Dependency Mappings

### New external dependencies
- **`rust-script`** (dev tool, not a Cargo dep) — confirmed installed at `~/.cargo/bin/rust-script` v0.21.0. Used to run `scripts/bump-version.rs` as a standalone script with inline cargo deps.
- **`semver` crate** — used by `bump-version.rs` via `rust-script`'s inline `//! [dependencies]` block. NOT added to auditah's `Cargo.toml` `[dependencies]`; it's a script-local dep.

### No new internal module dependencies
The bump flow is entirely external to auditah's Rust code (justfile + script + PKGBUILD). No `use` changes, no new modules.

### VCS dependency
- **git** (not Fossil) — auditah uses git; the recipe uses `git diff-index`, `git tag`, `git push`. jinn's Fossil-specific commands (`fossil branch current`, `fossil tag list`, `fossil tag add`) are NOT ported.

---

## Test Strategies

### Phase 1: `bump-version.rs`
- **Manual:** `rust-script scripts/bump-version.rs 0.1.0 patch` → must print `0.1.1` (no trailing newline).
- **Manual:** `rust-script scripts/bump-version.rs 0.1.0 minor` → `0.2.0`.
- **Manual:** `rust-script scripts/bump-version.rs 0.1.0 major` → `1.0.0`.
- **Manual:** `rust-script scripts/bump-version.rs 0.1 patch` → error "Invalid version" (exit 1).
- **Manual:** `rust-script scripts/bump-version.rs 0.1.0 foo` → error "Invalid bump level" (exit 1).

### Phase 2: `just bump` recipe
- **Dirty-tree abort:** make a trivial uncommitted change → `just bump patch` → must abort with "working tree has uncommitted changes".
- **Clean-tree success:** `just bump patch` on a clean tree → updates Cargo.toml + PKGBUILD, commits, tags, pushes.
- **Version flag:** after bump, `auditah --version` → `auditah 0.1.1`.
- **Tag presence:** after bump, `git tag --list 'v*'` includes `v0.1.1`.

### Phase 3: PKGBUILD
- **Build:** `just pkg` → produces `build/auditah-<ver>-1-x86_64.pkg.tar.zst` (no `-git`, no hashes).
- **Tag pin:** inspect `source=()` in the built PKGBUILD → contains `#tag=v$pkgver`.
- **No `pkgver()`:** grep the PKGBUILD → no `pkgver()` function.

### Phase 4: Baseline tag
- **Presence:** `git tag --list v0.1.0` → exists.
- **Remote:** `git ls-remote --tags origin v0.1.0` → exists on remote.

### Phase 5: Full verification
- `cargo build --tests` clean.
- `cargo clippy --tests -- -D warnings` clean.
- `cargo fmt --check` clean.
- `cargo nextest run` → all tests pass.
- Full workflow: `just bump patch && just pkg` → `build/auditah-0.1.1-1-x86_64.pkg.tar.zst` exists.

---

## Acceptance Criteria

1. `just bump patch` aborts with a clear error if the working tree has uncommitted changes.
2. `just bump patch` updates `Cargo.toml` `version` AND PKGBUILD `pkgver` to the new value, resets `pkgrel=1`, commits, tags `v<new>`, pushes commits + tags.
3. `just pkg` after a bump clones the tag from GitHub and produces `auditah-<ver>-1-x86_64.pkg.tar.zst` (no git hashes in the filename).
4. `auditah --version` reports the bumped version (no code change — clap reads `CARGO_PKG_VERSION`).
5. PKGBUILD `source` pins to `#tag=v$pkgver`; no `pkgver()` function remains.
6. `v0.1.0` exists as a baseline tag (local + remote).
7. `cargo build --tests`, `cargo clippy --tests`, `cargo fmt --check`, full suite clean.
8. Unchanged `just pkg` recipe still isolates to `./build/` (BUILDDIR semantics preserved).

---

## Test Cases

| # | Case | Expected |
|---|---|---|
| 1 | `rust-script scripts/bump-version.rs 0.1.0 patch` | prints `0.1.1` |
| 2 | `rust-script scripts/bump-version.rs 0.1.0 minor` | prints `0.1.2` |
| 3 | `rust-script scripts/bump-version.rs 0.1.0 major` | prints `1.0.0` |
| 4 | `rust-script scripts/bump-version.rs 0.1 patch` | error: "Invalid version" |
| 5 | `rust-script scripts/bump-version.rs 0.1.0 foo` | error: "Invalid bump level" |
| 6 | `just bump patch` with dirty tree | aborts: "working tree has uncommitted changes" |
| 7 | `just bump patch` with clean tree | updates Cargo.toml + PKGBUILD, commits, tags, pushes |
| 8 | After `just bump patch`, `auditah --version` | prints `auditah 0.1.1` |
| 9 | After `just bump patch`, `git tag` | includes `v0.1.1` |
| 10 | `just pkg` after bump | package named `auditah-0.1.1-1-x86_64.pkg.tar.zst` (no hashes) |
| 11 | `just pkg` output location | `build/auditah-0.1.1-1-x86_64.pkg.tar.zst` (not repo root) |
| 12 | Tag-pinned source fetch | makepkg clones `#tag=v0.1.1`, not `HEAD` |
