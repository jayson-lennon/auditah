# auditah

Obligation-aware license compliance and attribution tool for game development.

`reuse` models a license as an *identifier* and audits only the declarative
layer (every file declares an SPDX ID; every ID has a text file). auditah starts
from a different premise: **a license is a set of obligations and permissions.**
An auditor that can neither carry nor surface obligation data cannot audit
compliance for any obligation-bearing license. auditah stores the full term set
per license, verifies obligations are *fulfilled*, and surfaces the ones that
can't be auto-checked as explicit flags.

## Quickstart

```sh
# Audit the current project for license compliance.
auditah audit

# Generate CREDITS.md from attribution sidecars and manifests.
auditah credits

# Scaffold a sidecar for one asset (interactive prompts).
auditah add path/to/sword.glb

# Cover an entire asset pack directory with one manifest.
auditah init-pack path/to/pack --license CC0-1.0 --author Quaternius
```

## Project config — `auditah.toml`

Placed at the project root:

```toml
commercial_project = true   # FAILs assets with effective allows_commercial_use = false

exclude = [
    "src/**",        # your first-party source
    "*.zip",         # exclude archives (default already excludes these)
    "vendor/**",     # anything you don't want audited
]
```

When `commercial_project = true`, any asset whose effective terms set
`allows_commercial_use = false` fails the audit.

## Attribution that travels with the asset

The core principle: **the asset plus its attribution is one unit.** No root
table to drift when files move or get renamed. License info moves with the file.

### Sidecars — `<asset>.attr.toml`

A sidecar lives next to a single asset and covers that one file:

```toml
# sword.glb.attr.toml
title   = "Gunny Sack"
author  = "Oliver Herklotz"
year    = 2019
license = "CC-BY-3.0"
source  = "https://poly.pizza/m/download/Gunny-Sack/..."
modified = false
```

Move or rename `sword.glb` and its `.attr.toml` together — no config edits needed.

### Directory manifests — `manifest.toml`

A manifest covers its directory and all subdirectories, ideal for asset packs
where every file shares one license:

```toml
# pack/manifest.toml
title   = "Modular Dungeons Pack"
author  = "Quaternius"
year    = 2022
license = "CC0-1.0"
source  = "https://poly.pizza"
```

### Resolution precedence (most specific wins)

1. **Sidecar** `<asset>.attr.toml` — overrides everything.
2. **Nearest ancestor `manifest.toml`** — subdirectory manifests override
   parent manifests.
3. **None** — `audit` fails the asset as unlicensed.

A directory manifest holds **exactly one** license/terms block. If a single file
in a covered directory differs, it gets its own sidecar. There is no
multi-license root table.

## License terms and overrides

Each license in the registry declares its obligations and permissions:

```toml
[terms]
requires_attribution       = true    # obligation: you MUST do this
requires_license_notice    = false
requires_source_disclosure = false
requires_share_alike       = false
requires_modification_notice = true
allows_commercial_use      = true    # permission: you MAY do this
allows_modifications       = true
```

**Effective terms** for an asset are the license's terms, with optional
per-asset `[overrides]` applied. Overrides are for non-standard arrangements on
a specific asset (e.g. an author grants CC-BY but forbids commercial use):

```toml
# fanfare.ogg.attr.toml
title   = "Fanfare"
author  = "Musician"
year    = 2021
license = "CC-BY-3.0"
source  = "https://example.com"

[overrides]
allows_commercial_use = false   # opt this asset out of commercial use
```

> **Override-semantics caveat:** in v1, if any override field is set, the
> **entire** term set is taken from the override block, not merged field-by-field
> with the license. Specify all the fields that matter for that asset. This will
> become field-level merging in a future version.

## What the audit checks

| Check | Severity |
|---|---|
| Asset has no sidecar and no enclosing manifest | **FAIL** — unlicensed asset |
| Orphan sidecar (its asset file is gone) | **FAIL** |
| `license` id not in the registry | **FAIL** — unknown license |
| `requires_attribution` but missing title/author/source | **FAIL** — incomplete attribution |
| `allows_commercial_use = false` and `commercial_project = true` | **FAIL** |
| `allows_modifications = false` and `modified = true` | **FAIL** — no-derivatives |
| `requires_share_alike`, `requires_source_disclosure`, `requires_license_notice` | **FLAG** — needs human review |

`audit` exits non-zero on any FAIL. FLAGs are reported but don't block.

## License registry

auditah ships embedded definitions for CC0-1.0, CC-BY-3.0, MIT, and OFL-1.1. A
project-local `licenses/` directory at the project root can override embedded
entries (same id) or add new ones (e.g. `LicenseRef-StudioEULA`). Each license
definition is a TOML file matching the `[terms]` shape above, plus `id`, `name`,
`url`, and `text`.

## Building

```sh
just build     # cargo build
just test      # cargo test
just clippy    # cargo clippy --all-targets -- -D warnings
```
