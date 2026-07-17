# auditah

Obligation-aware license compliance and attribution tool for game development.

`reuse` models a license as an _identifier_ and audits only the declarative
layer (every file declares an SPDX ID; every ID has a text file). auditah starts
from a different premise: **a license is a set of obligations and permissions.**
An auditor that can neither carry nor surface obligation data cannot audit
compliance for any obligation-bearing license. auditah stores the full term set
per license, verifies obligations are _fulfilled_, and surfaces the ones that
can't be auto-checked as explicit action items.

## Quickstart

```sh
# Audit the current project for license compliance (exit 1 on findings, 2 on error).
auditah audit

# Generate CREDITS.md, NOTICES.md, BOM.md in one shot (audit-gated: refuses on a failing project).
auditah generate

# Scaffold a sidecar for one asset (interactive prompts for any field not passed on the CLI).
auditah sidecar path/to/sword.glb --license CC-BY-4.0 --author "Author Name"

# Scaffold a license definition: well-known SPDX id extracts text + grid from the embedded corpus.
auditah license MIT

# Cover an entire asset pack directory with one `_manifest.toml`.
auditah init-pack path/to/pack --license CC0-1.0 --author "Quaternius"
```

## Commands

| Command     | What it does                                                                                       |
| ----------- | -------------------------------------------------------------------------------------------------- |
| `audit`     | Audit license compliance of assets. Exit 1 if any FAIL finding, 2 on technical error.              |
| `sidecar`   | Scaffold an `<asset>.attr.toml` sidecar for a single asset.                                        |
| `license`   | Scaffold a license definition in `LICENSES/` (`.toml` grid + `.txt` text).                         |
| `generate`  | Write CREDITS.md, NOTICES.md, BOM.md. Runs an audit gate first; no artifacts on a failing project. |
| `init`     | Write a commented `auditah.toml` at the project root (refuses overwrite unless `--force`).         |
| `ack`      | Acknowledge a manual-review license id (adds to `manual_review_acknowledged`).                      |
| `init-pack` | Write a directory `_manifest.toml` covering a folder and its subdirs.                              |

## Project config (`auditah.toml`)

Placed at the project root:

```toml
# FAILs assets whose effective allows_commercial_use = false.
commercial_project = true

# FAILs assets whose effective allows_redistribution = false.
redistributes_assets = false

# SPDX ids whose manual_review obligation has been acknowledged.
manual_review_acknowledged = [
    "LicenseRef-StudioEULA",
]

# Additional glob patterns to exclude (merged after the built-in defaults).
# Matched against paths relative to the project root.
exclude = [
    "src/**",
    "vendor/**",
]
```

`commercial_project = true`: any asset whose effective terms set
`allows_commercial_use = false` FAILs the audit.

`redistributes_assets = true`: set this if you re-host or resell the raw asset
itself (not just shipping it embedded in a product). Any asset whose effective
terms set `allows_redistribution = false` then FAILs.

`manual_review_acknowledged`: a license with `manual_review = true` FAILs until
its id is listed here. Acknowledgment is permanent and silent.

Configuration is optional: an absent `auditah.toml` yields defaults (all flags
false, both lists empty).

### Scaffolding config: `init` and `ack`

Generate the commented template above (defaults) in the current directory:

```
auditah init            # writes ./auditah.toml; refuses if it exists
auditah init --force    # overwrite an existing file
auditah init --root path/to/project
```

Acknowledge a manual-review license id (suppresses its `ManualReviewRequired`
finding) without hand-editing the toml:

```
auditah ack LicenseRef-StudioEULA   # creates auditah.toml if absent, else appends
auditah ack MIT CC-BY-3.0            # acknowledge several ids at once
```

`ack` edits `manual_review_acknowledged` in-place via a lossless TOML AST, so
existing comments, formatting, and key order are preserved. Re-acknowledging an id
already present is a no-op (idempotent). Ids unknown to both `LICENSES/` and the
well-known SPDX corpus print a warning on stderr but are still written (fail-open).

## Attribution that travels with the asset

The core principle: **the asset plus its attribution is one unit.** No root
table to drift when files move or get renamed. License info moves with the file.

### Sidecars (`<asset>.attr.toml`)

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

Move or rename `sword.glb` and its `.attr.toml` together. No config edits needed.

### Directory manifests (`_manifest.toml`)

A manifest covers its directory and all subdirectories, ideal for asset packs
where every file shares one license:

```toml
# pack/_manifest.toml
title   = "Example Asset Pack"
author  = "Jane Doe"
year    = 2024
license = "CC0-1.0"
source  = "https://example.com"
```

### Resolution precedence (most specific wins)

1. **Sidecar** `<asset>.attr.toml`: overrides everything.
2. **Nearest ancestor `_manifest.toml`**: subdirectory manifests override
   parent manifests.
3. **None**: `audit` fails the asset as unlicensed.

A directory manifest holds **exactly one** license/terms block. If a single file
in a covered directory differs, it gets its own sidecar. There is no
multi-license root table.

## License terms and overrides

Each license in the registry declares its obligations and permissions:

```toml
[terms]
# You MUST attribute the author (requires title + author + source).
requires_attribution = true
# You MUST reproduce the license text in your distribution.
requires_license_notice = false
# You MUST offer corresponding source code on distribution (tracked in BOM, no finding).
requires_source_disclosure = false
# allowed | disallowed | share-alike
derivatives = "allowed"
# If modified = true, you MUST state the modification in credits.
requires_modification_notice = true
# You MAY use this commercially.
allows_commercial_use = true
# You MAY redistribute (re-host/resell) the asset itself.
allows_redistribution = true
# License-only escape hatch: FAILs audit until the id is in manual_review_acknowledged.
manual_review = false
```

**Effective terms** for an asset are the license's terms, with optional
per-asset `[overrides]` applied. Overrides are for non-standard arrangements on
a specific asset (e.g. an author grants CC-BY but forbids commercial use):

```toml
# fanfare.ogg.attr.toml
#
title   = "Fanfare"
author  = "Musician"
year    = 2021
license = "CC-BY-3.0"
source  = "https://example.com"

# Overrides merge field-by-field onto the license's terms. Set only the fields
# that differ for this asset; everything else inherits from the license.
# manual_review is license-only and cannot be overridden.
[overrides]
# Opt this asset out of commercial use.
allows_commercial_use = false
```

## What the audit checks

| Check                                                             | Severity                               |
| ----------------------------------------------------------------- | -------------------------------------- |
| Asset has no sidecar and no enclosing manifest                    | **FAIL** (unlicensed asset)            |
| Orphan sidecar (its asset file is gone)                           | **FAIL**                               |
| `license` id not in the registry                                  | **FAIL** (unknown license)             |
| `requires_attribution` but missing title/author/source            | **FAIL** (incomplete attribution)      |
| `allows_commercial_use = false` and `commercial_project = true`   | **FAIL**                               |
| `allows_redistribution = false` and `redistributes_assets = true` | **FAIL** (no redistribution)           |
| `derivatives = "disallowed"` and `modified = true`                | **FAIL** (no-derivatives)              |
| Referenced license has no `LICENSES/<id>.txt`                     | **FAIL** (missing license text)        |
| `manual_review = true` and not in `manual_review_acknowledged`    | **FAIL** (requires human review + ack) |

Some obligations aren't auto-verifiable by `audit` and so produce **no finding**;
they're handled by the distribution artifacts instead:

- `requires_source_disclosure`: surfaced as an action item in `BOM.md` (no finding).
- `requires_license_notice`: satisfied automatically by `NOTICES.md` via `generate`.
- `derivatives = "share-alike"`: the boolean grid can't verify relicensing; no
  separate finding. Track it manually.

`audit` exits non-zero on any FAIL. There are no non-blocking warnings.

### `LICENSES/` directory and `license`

Every license referenced by any asset must have a full-text file at
`LICENSES/<id>.txt` (e.g. `LICENSES/MIT.txt`, `LICENSES/LicenseRef-StudioEULA.txt`).
These are **required**, not optional. `audit` FAILs any referenced license whose
text file is missing. The on-disk files are authoritative: you can edit them (e.g.
trim a license's boilerplate) and auditah will respect your edits.

```sh
# Scaffold a well-known SPDX license: extracts canonical text + authored grid
# from the embedded corpus into LICENSES/.
auditah license MIT

# Scaffold a custom LicenseRef-* license (default_fail() placeholder grid).
auditah license --custom StudioEULA
```

**Workflow for a custom / bespoke license:**

1. Run `auditah license --custom StudioEULA`. It writes a `default_fail()`
   placeholder grid at `LICENSES/LicenseRef-StudioEULA.toml` (every permission
   false, `manual_review = true`).
2. Edit the grid to fill in the real terms, and drop the legal text alongside at
   `LICENSES/LicenseRef-StudioEULA.txt`.
3. Add the id to `manual_review_acknowledged` in `auditah.toml` when you've
   reviewed it.
4. Reference the license by id (`license = "LicenseRef-StudioEULA"`) in sidecars
   and manifests. The text file is present and audit passes.
