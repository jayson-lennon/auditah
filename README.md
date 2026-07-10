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
derivatives                = "allowed"  # allowed | disallowed | share-alike
requires_modification_notice = true
allows_commercial_use      = true    # permission: you MAY do this
allows_redistribution      = true
manual_review              = false   # license-only: surface for human review
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

> **Override semantics:** overrides merge field-by-field onto the license's
> terms. Set only the fields that differ for that asset; everything else
> inherits from the license. `manual_review` is license-only and cannot be
> overridden.

## What the audit checks

| Check | Severity |
|---|---|
| Asset has no sidecar and no enclosing manifest | **FAIL** — unlicensed asset |
| Orphan sidecar (its asset file is gone) | **FAIL** |
| `license` id not in the registry | **FAIL** — unknown license |
| `requires_attribution` but missing title/author/source | **FAIL** — incomplete attribution |
| `allows_commercial_use = false` and `commercial_project = true` | **FAIL** |
| `allows_redistribution = false` and `redistributes_assets = true` | **FAIL** — no redistribution |
| `derivatives = "disallowed"` and `modified = true` | **FAIL** — no-derivatives |
| Referenced license has no `LICENSES/<id>.txt` | **FAIL** — missing license text |
| `derivatives = "share-alike"`, `requires_source_disclosure`, `requires_license_notice` | **FLAG** — needs human review |
| `manual_review = true` and not in `manual_review_acknowledged` | **FAIL** — requires human review + ack |

`audit` exits non-zero on any FAIL. FLAGs are reported but don't block.

## License registry

auditah ships embedded definitions for CC0-1.0, CC-BY-3.0, MIT, and OFL-1.1. A
project-local `licenses/` directory at the project root can override embedded
entries (same id) or add new ones (e.g. `LicenseRef-StudioEULA`). Each license
definition is a TOML file matching the `[terms]` shape above, plus `id`, `name`,
`url`, and `text`.

### `LICENSES/` directory and `init-licenses`

Every license referenced by any asset must have a full-text file at
`LICENSES/<id>.txt` (e.g. `LICENSES/MIT.txt`, `LICENSES/LicenseRef-StudioEULA.txt`).
These are **required**, not optional — `audit` FAILs any referenced license whose
text file is missing. The on-disk files are authoritative: you can edit them (e.g.
trim a license's boilerplate) and auditah will respect your edits.

```sh
# Write LICENSES/<id>.txt for every license in the registry.
# Idempotent: existing files with matching content are skipped. Divergent files
# (human-edited) cause an error — on-disk text is never silently clobbered.
auditah init-licenses
```

**Workflow for a custom / bespoke license:**

1. Author the registry definition at `licenses/LicenseRef-StudioEULA.toml` (same
   shape as above, with your full `text` inline).
2. Run `auditah init-licenses` — it writes `LICENSES/LicenseRef-StudioEULA.txt`
   from the `text` field of your `.toml`.
3. Reference the license by id (`license = "LicenseRef-StudioEULA"`) in sidecars
   and manifests. The text file is now present and audit passes.

## Building

```sh
just build     # cargo build
just test      # cargo test
just clippy    # cargo clippy --all-targets -- -D warnings
```
