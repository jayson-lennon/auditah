# auditah — Context-Rich Specification

## Problem

`reuse` models a license as an *identifier* and audits only the declarative
layer: every file declares an SPDX ID, and every ID has a text file in
`LICENSES/`. A license is not an identifier — **a license is a set of
obligations and permissions.** An auditor that can neither carry nor surface
obligation data cannot audit compliance for any obligation-bearing license; it
silently implies the rest is handled when it isn't.

Gamedev additionally needs attribution data (title, author, source) to *travel
with the asset* across moves/renames, and to be emitted into a human-facing
`CREDITS.md`. `reuse` drops extra SPDX tags from its `spdx` output and provides
no credits generation, so it cannot serve as the source of truth for
attribution.

## Solution

A standalone Rust CLI, **auditah**, that:

- Stores attribution + license info in **sidecars (`<name>.attr.toml`)** and
  **directory manifests (`manifest.toml`)** that move with their assets — no
  root table to drift from a file path.
- Maintains an **obligation-aware license registry** (`requires_*` /
  `allows_*` terms) embedded in the binary, extended per-project via a
  project-local `licenses/` directory.
- Walks every file (minus excludes), resolves each asset's config by
  precedence, and **audits obligation fulfillment** — FAILing where checkable,
  FLAGging where human review is required.
- Emits a grouped `CREDITS.md` for attribution-bearing licenses.

## Dialectical Outcomes (Why)

| Decision | Reasoning | Rejected Alternative |
|---|---|---|
| Custom tool vs. extending `reuse` | `reuse` cannot carry or surface obligation data (verified: it drops `SPDX-FileComment`/`SPDX-AttributionText` from `reuse spdx` output). Attribution fundamentally travels with the asset, not in an external table. Building fresh is the only honest path. | Repurposing `SPDX-FileCopyrightText` as a free-form carrier (hacky); separate `CREDITS.toml` root table (drifts on move). |
| `.attr.toml` sidecar suffix | A sidecar is data the tool parses, not display markup. TOML parses cleanly and preserves comments via `toml_edit`. | `.attr.html` (originally requested, then confirmed a typo); `.license` (collides with `reuse`). |
| `manifest.toml` directory file | Generic, clear name; no collision with sidecars. | `ATTRIBUTION.toml`, `LICENSE.toml`, reusing `.attr.toml` as a dir file. |
| Single license/terms block per manifest | Huge simplification. A differing file gets its own sidecar. Sidecar > nearest manifest > FAIL. | REUSE.toml-style `[[annotations]]` array of different licenses per file group — drifts back toward a root table. |
| Boolean terms + manual-review FLAGs | Real licenses have conditional obligations (GPL source-on-distribution, CC-BY attribution specifics) that no local tool can auto-verify. Honest FLAGs beat false confidence. | Richer obligation structs with `condition` fields (more surface area, same unmodelability). |
| Walk-everything enumeration + excludes | Safest: catches *missing* licenses. Explicit excludes keep scope tight. | Opt-in (only files with sidecars) — never catches unlicensed assets, defeats the auditor. |
| Embedded license registry (`include_str!`) | Single binary, zero runtime file deps. Project `licenses/` merges custom/`LicenseRef-*` on top. | Shipped-as-files (two artifacts); fully project-local (every project reauthors CC0). |
| Assets-only scope (v1) | Third-party binary assets are where attribution matters; own code is own copyright. | Inline header support for `.rs`/`.gd` — deferred to a later phase. |

## Relevant Files (Where)

Project root: `/mnt/zed/repos/auditah` (empty git repo, no commits yet).

```
auditah/
├── Cargo.toml
├── README.md
├── justfile
├── auditah.toml                        # sample project config (committed as example)
├── src/
│   ├── main.rs                         # clap CLI entry, dispatches subcommands
│   ├── config.rs                       # auditah.toml parse (commercial_project, [exclude])
│   ├── model/
│   │   ├── mod.rs
│   │   ├── terms.rs                    # LicenseTerms (requires_*/allows_*), EffectiveTerms
│   │   ├── attribution.rs             # AttributionRecord (sidecar/manifest payload)
│   │   └── license.rs                 # LicenseRegistryEntry (id, name, url, text, terms)
│   ├── registry/
│   │   ├── mod.rs                      # LicenseRegistry: embedded + project-local merge
│   │   └── embedded.rs                 # include_str! of bundled license definitions
│   ├── embedded_licenses/              # source files compiled in via include_str!
│   │   ├── CC0-1.0.toml
│   │   ├── CC-BY-3.0.toml
│   │   ├── MIT.toml
│   │   └── OFL-1.1.toml
│   ├── discovery/
│   │   ├── mod.rs                      # AssetEnumerator (walk + excludes)
│   │   └── resolver.rs                 # ConfigResolver (sidecar → manifest → FAIL)
│   ├── services/
│   │   ├── mod.rs                      # Services container (DI), service wrappers
│   │   ├── fs.rs                       # FsService trait + impl (read/write/list)
│   │   └── registry.rs                 # RegistryService trait + impl
│   ├── audit/
│   │   ├── mod.rs                      # AuditCtx, run_audit orchestration
│   │   ├── checks.rs                   # coverage, resolution, obligation checks
│   │   └── report.rs                   # Finding, Severity (Fail/Flag), grouped report
│   ├── credits/
│   │   ├── mod.rs                      # credits orchestration
│   │   └── render.rs                   # CREDITS.md rendering, grouping by author
│   └── cli/
│       ├── mod.rs
│       ├── audit_cmd.rs
│       ├── credits_cmd.rs
│       ├── add_cmd.rs
│       └── init_pack_cmd.rs
└── tests/                              # integration tests (temptree-based, see Test Strategies)
    ├── audit_flow.rs
    ├── credits_flow.rs
    ├── discovery_precedence.rs
    └── migration.rs
```


## Key Code Context (What)

These are the core type definitions the implementation is built around. They
must exist before any phase that depends on them.

### LicenseTerms (src/model/terms.rs)

```rust
/// Obligations and permissions of a license. Declared per-license in the
/// registry; overridable per-asset via [overrides].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LicenseTerms {
    /// You MUST attribute the author (requires title + author + source).
    pub requires_attribution: bool,
    /// You MUST reproduce the license text in your distribution.
    pub requires_license_notice: bool,
    /// You MUST offer corresponding source code on distribution.
    pub requires_source_disclosure: bool,
    /// You MUST license derivatives under the same terms. Auto-unverifiable → FLAG.
    pub requires_share_alike: bool,
    /// If modified=true, you MUST state the modification in credits.
    pub requires_modification_notice: bool,
    /// You MAY use this commercially.
    pub allows_commercial_use: bool,
    /// You MAY create derivatives.
    pub allows_modifications: bool,
}
```

### AttributionRecord (src/model/attribution.rs)

```rust
/// One asset's attribution + license config. Lives in a sidecar or manifest.
/// In a manifest, it applies to the whole dir subtree (minus overrides).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttributionRecord {
    pub title: String,
    pub author: String,
    pub year: Option<u16>,
    pub license: String,          // SPDX ID or LicenseRef-*
    pub source: String,           // URL the asset was obtained from
    pub modified: bool,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub overrides: Option<LicenseTerms>,
}
```

### LicenseRegistryEntry (src/model/license.rs)

```rust
/// One entry in the license registry. Embedded in the binary for common
/// licenses; project-local for custom/`LicenseRef-*`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LicenseRegistryEntry {
    pub id: String,               // "CC-BY-3.0", "LicenseRef-StudioEULA"
    pub name: String,
    pub url: String,
    pub text: String,             // full license text (embedded via include_str!)
    pub terms: LicenseTerms,
    pub notes: Option<String>,
}
```

### EffectiveTerms computation (src/model/terms.rs)

```rust
/// The terms that actually apply to an asset: registry terms, then apply
/// any per-asset [overrides]. An override of `None` means "use license defaults."
pub fn effective_terms(
    registry: &LicenseTerms,
    overrides: Option<&LicenseTerms>,
) -> LicenseTerms {
    match overrides {
        None => registry.clone(),
        // Overrides replace wholesale for v1 (whole-block semantics).
        Some(o) => o.clone(),
    }
}
```

> **Gotcha — override semantics (v1):** ~~overrides replace the terms block
> wholesale. Partial override (field-by-field merge) is deferred; if a
> per-asset `[overrides]` block is present, every `requires_*`/`allows_*`
> field must be specified.~~
>
> **DIVERGENCE (task tg8u):** Implemented as **partial/merge overrides** via a
> dedicated `Overrides` struct of `Option<bool>` fields. Only set fields replace
> the base; unset fields inherit. Rationale: wholesale replacement would force
> redeclaring all 7 fields to flip one (unusable), and test case 11 ("asset
> `[overrides]` flips `allows_commercial_use`") sets only one field. Matches the
> spec algorithm description ("then apply asset overrides"). `effective_terms`
> signature changed to `(base: &LicenseTerms, overrides: &Overrides) -> LicenseTerms`.
> field must be specified. Document this in `add`/`init-pack` help and the
> README. A future phase may introduce a merge strategy.

### Services container (src/services/mod.rs)

```rust
/// Dependency-injection container. Constructed once in main (real backends)
/// or in tests (fakes). Cheap to clone; every field is a service wrapper.
#[derive(Debug, Clone)]
pub struct Services {
    pub fs: FsService,
    pub registry: RegistryService,
}
```

Each service is a trait + wrapper following the skill pattern:
```rust
#[derive(Debug, Clone)]
pub struct FsService {
    backend: Arc<dyn FsBackend>,
}
impl FsService {
    pub fn new(backend: Arc<dyn FsBackend>) -> Self { Self { backend } }
    pub fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>> {
        self.backend.read_to_string(path)
    }
    // ...write, list_dir, walk, exists...
}
pub trait FsBackend {
    fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>>;
    fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>>;
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;
    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;
    fn exists(&self, path: &Path) -> bool;
    fn name(&self) -> &'static str;
}
```

### Finding / report (src/audit/report.rs)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity { Fail, Flag }

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub asset: PathBuf,
    pub code: FindingCode,        // enum of failure categories
    pub message: String,
}

#[derive(Debug, Default)]
pub struct AuditReport {
    pub findings: Vec<Finding>,
}
impl AuditReport {
    pub fn has_failures(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Fail)
    }
}
```

## Implementation Algorithm (How)

### Discovery + resolution flow (audit)

```
1. Load auditah.toml (commercial_project, [exclude] globs). Merge with default excludes.
2. Enumerate: walk root recursively. For each path:
     a. Skip if it matches any exclude glob.
     b. Skip if it is a sidecar (*.attr.toml) or a manifest.toml itself.
   → result: Vec<PathBuf> of candidate assets.
3. For each candidate asset, resolve config:
     a. If `<asset>.attr.toml` exists adjacent → parse it, use it (SIDECAR).
     b. Else, walk up parent dirs; first `manifest.toml` found → use it (MANIFEST).
     c. Else → UNCOVERED (will FAIL in checks).
4. For each sidecar/manifest discovered, record it; detect orphan sidecars
   (sidecar whose `<asset>` doesn't exist) → FAIL orphan.
5. Build list of (asset_path, resolved AttributionRecord, resolution_source).
```

### Audit checks (per asset)

```
For each asset in the enumerated list:
  1. COVERAGE: if resolution_source == NONE → FAIL "unlicensed asset".
  2. RESOLUTION: if record.license not in registry → FAIL "unknown license".
  3. Compute effective_terms = registry[license].terms overridden by record.overrides.
  4. OBLIGATIONS (auto-checkable):
     - effective.requires_attribution AND (title/author/source empty)
         → FAIL "incomplete attribution".
     - NOT effective.allows_commercial_use AND config.commercial_project
         → FAIL "not licensed for commercial use".
     - NOT effective.allows_modifications AND record.modified
         → FAIL "modified under no-derivatives license".
  5. OBLIGATIONS (manual-review):
     - effective.requires_share_alike → FLAG "share-alike: confirm compatible".
     - effective.requires_source_disclosure → FLAG "source disclosure required".
     - effective.requires_license_notice → FLAG "license notice required".
  6. MODIFICATION NOTICE:
     - effective.requires_modification_notice AND record.modified
         → mark record for notice emission in credits (not a failure).

Exit code: non-zero if any Fail; zero otherwise. Flags print but don't fail.
```

### Credits generation flow

```
1. Run discovery + resolution (same as audit).
2. Filter: keep only assets whose effective.requires_attribution is true
   (CC0 and other attribution-free licenses are omitted by default).
3. Group by author. Within author, sort by title.
4. Render to CREDITS.md:
     ## <Author>
     - **<Title>** (<Year>) — <License Name> — <source URL>
       (modified from original)              ← only if requires_modification_notice AND modified
5. Write to CREDITS.md (path configurable; default project root).
```

### License registry merge

```
1. Load embedded registry definitions (include_str! → parse each TOML).
2. If <project>/licenses/ exists, parse each *.toml; merge by id.
   - Project entry with same id as embedded → project wins (override).
   - Project entry with new id (e.g. LicenseRef-*) → added.
3. Registry is a HashMap<id, LicenseRegistryEntry>.
```

## Anti-Goals (Out of Scope)

- **No inline header support** for code/text files (`.rs`, `.gd`, `.gdshader`).
  v1 is binary assets only.
- **No multi-license manifests.** A `manifest.toml` holds exactly one
  `AttributionRecord` for its dir + subdirs. A differing file requires a
  sidecar. No `[[annotations]]` array.
- **No partial override merging.** `[overrides]` replaces terms wholesale for
  v1; all fields must be present if the block is used.
- **No SPDX SBOM export.** Deferred to a future phase; never a storage format.
- **No condition/obligation-state machine.** Conditionals (GPL
  source-on-distribution) are FLAGs, not auto-verified logic.
- ~~**No re-licensing of existing assets beyond the 3 in migration.**~~ (DROPPED: no real assets in this plan.)
- **No Godot `.tres`/`.json` credits output.** `CREDITS.md` only for v1.
- **No network access.** License text is embedded or project-local, never
  fetched at runtime.

## Edge Cases & Gotchas

1. **Orphan sidecar:** `<asset>.attr.toml` exists but `<asset>` was deleted →
   FAIL (prevents stale attribution lingering after asset removal).
2. **Override semantics:** whole-block replacement, not field merge. Document
   clearly; `add` should scaffold a full block when overrides are requested.
3. **Subdir manifest vs parent manifest:** subdir `manifest.toml` wins for its
   subtree. Resolution walks up from the asset, first hit wins.
4. **Sidecar vs manifest in same dir:** sidecar wins unconditionally for that
   one file; manifest still applies to siblings.
5. **Filename with spaces / unicode** (`Gunny Sack.glb`): sidecar naming must
   append `.attr.toml` to the full filename including spaces. Test this.
6. **Archives (`.zip`, `.tar`):** excluded by default (they're containers, not
   the assets themselves). Configurable.
7. **Walking the tool's own files:** default excludes cover `.git`, `target/`,
   `*.lock`, `Cargo.*`, `auditah.toml`, `CREDITS.md`, `*.attr.toml`,
   `manifest.toml`, `licenses/`, `LICENSES/`.
8. **`LicenseRef-*` ids:** always project-local; never embedded. The embedded
   set is the common SPDX set only.
9. **CC-BY version ambiguity:** original source said `[CC-BY]`; resolved to
   `CC-BY-3.0` per user. The registry ships CC-BY-3.0; CC-BY-4.0 would be a
   project-local addition if needed.
10. **`requires_share_alike` is project-output-binding, not asset-verifiable:**
    the auditor cannot inspect the project's distribution license, so it FLAGs.

## Navigation Anchors

- **CLI entry:** `src/main.rs` → clap dispatch → `src/cli/*_cmd.rs`.
- **Audit centerpiece:** `src/audit/mod.rs::run_audit(services, config, root)`.
- **Discovery:** `src/discovery/mod.rs::AssetEnumerator::enumerate(root, excludes)`.
- **Resolution:** `src/discovery/resolver.rs::ConfigResolver::resolve(asset_path)`.
- **Registry:** `src/registry/mod.rs::LicenseRegistry::load(project_root)`.
- **Credits:** `src/credits/mod.rs::generate_credits(services, config, root)`.
- **DI root:** `src/services/mod.rs::Services` constructed in `main.rs`.

## Dependency Mappings

| Crate | Purpose |
|---|---|
| `clap` (derive) | Subcommand CLI parsing. |
| `serde` + `serde_derive` | Serialize/deserialize config, attribution, license TOML. |
| `toml` | Parse embedded/project license registry, `auditah.toml`. |
| `toml_edit` | Read/write sidecars and manifests **preserving comments**. |
| `error_stack` | `Report<T>` error reporting with attached context. |
| `wherror` | `#[error(debug)]` error types; no manual Display impls. |
| `walkdir` | Recursive directory walk for enumeration. |
| `globset` | Glob matching for `[exclude]` patterns. |
| `spdx` | License-ID validation (is `CC-BY-3.0` a valid expression?). |
| `derive_more` | `Debug` derives for service wrappers (`#[debug(...)]`). |
| `tempfile` | Real-filesystem integration tests (temp dirs). |
| `temptree` | Build arbitrary test directory trees declaratively. |
| `rstest` | Parameterized unit tests. |

## Test Strategies

### Unit tests (in-module, BDD, `rstest`)

- **LicenseTerms / effective_terms:** pure-function tests; `rstest` over
  (registry_terms, override, expected) triples. No filesystem.
- **Audit checks:** given a constructed `(AttributionRecord, LicenseTerms,
  config)` → assert `Finding` set. Pure; no services needed.
- **Credits render:** given `Vec<(AttributionRecord, EffectiveTerms)>` → assert
  exact `CREDITS.md` string. Snapshot-style with explicit expected string.

### Integration tests (`tests/`, using `tempfile` + `temptree`)

- **Discovery + resolution precedence** (`tests/discovery_precedence.rs`):
  Build a temp tree with sidecars + nested manifests; inject a *real* `FsService`
  (real backend) rooted at the temp dir; assert resolution picks sidecar >
  subdir manifest > parent manifest.
- **Audit flow** (`tests/audit_flow.rs`): each acceptance criterion from the
  table below is a `temptree`-built scenario → run `run_audit` → assert
  findings/severity. One behavior per test (BDD naming).
- **Credits flow** (`tests/credits_flow.rs`): temp tree → generate credits →
  assert CREDITS.md content incl. grouping, CC0 suppression, modification
  notices.
- ~~**Migration** (`tests/migration.rs`): copy the 3 real source files...~~ DROPPED:
  no real assets in this plan. Each phase verifies against synthetic
  `temptree` trees instead.

### Service fakes

Unit/integration tests inject a `FakeFsBackend` (in-memory map of path→content)
into `Services` where a real temp dir isn't needed. Where real filesystem
behavior (walk, globs, permissions) must be exercised, use `temptree` + the real
backend.

## Acceptance Criteria

1. `auditah audit` exits non-zero with a clear FAIL on any uncovered asset,
   unknown license, incomplete attribution, commercial-use violation, or
   no-derivs modification.
2. `auditah audit` passes (clean) only when every walked non-excluded asset has
   a resolvable config and all checkable obligations are met.
3. Moving or renaming an asset **and its sidecar** requires no config edits;
   `audit` still passes.
4. A subdirectory `manifest.toml` overrides its parent; a `*.attr.toml`
   overrides the nearest manifest.
5. `auditah credits` produces a `CREDITS.md` grouped by author, CC0 omitted,
   modification notices present where required.
6. An asset with `allows_commercial_use = false` (effective) FAILS `audit` when
   `commercial_project = true`.
7. All core logic unit-tested via a `Services`-injected fake backend; no real
   filesystem required for unit tests.

## Test Cases

| # | Scenario | Expected |
|---|---|---|
| 1 | Asset with no sidecar, no enclosing manifest, not excluded | FAIL: unlicensed asset |
| 2 | Orphan sidecar (asset file deleted) | FAIL: orphan sidecar |
| 3 | Asset covered by nearest `manifest.toml` | PASS (uses manifest terms) |
| 4 | Sidecar present in a manifest-covered dir | Sidecar wins; manifest ignored for that file |
| 5 | Subdir manifest vs parent manifest | Subdir wins for its subtree |
| 6 | `license = "MIT"` with registry entry | PASS |
| 7 | `license = "Nonexistent"` with no registry entry | FAIL: unknown license |
| 8 | `requires_attribution` + missing `source` | FAIL: incomplete attribution |
| 9 | `allows_commercial_use=false` + `commercial_project=true` | FAIL: not licensed for commercial use |
| 10 | `allows_modifications=false` + `modified=true` | FAIL: modified under no-derivatives |
| 11 | `requires_share_alike=true` | FLAG: needs human review (not FAIL) |
| 12 | Asset `[overrides]` flips `allows_commercial_use` | Effective terms reflect override; checked accordingly |
| 13 | Asset excluded via `[exclude]` glob | Not audited; no FAIL |
| 14 | CC0 asset | PASS; omitted from `credits` output |
| 15 | CC-BY asset + `modified=true` + `requires_modification_notice=true` | `credits` emits modification notice |
| 16 | Move asset + sidecar together | No config edit needed; `audit` PASS |

## Phases

### Phase 1 — Scaffold
Cargo project with `clap` subcommand skeleton (`audit`, `credits`, `add`,
`init-pack`); `wherror`/`error_stack` wiring; `Services` container + `FsBackend`
trait + real impl; README + justfile stubs; `derive_more` Debug setup. Verify:
`cargo build` succeeds, `auditah --help` lists subcommands, `Services` can be
constructed with a fake backend in a unit test.

### Phase 2 — License Registry
Define `LicenseTerms`, `LicenseRegistryEntry`, `AttributionRecord` types;
implement `LicenseRegistry::load` with embedded (`include_str!`) definitions for
CC0-1.0, CC-BY-3.0, MIT, OFL-1.1 + project-local `licenses/` merge;
`effective_terms()` computation. Verify: unit tests over registry lookup,
override application, unknown-id rejection.

### Phase 3 — Discovery + Parsing
`AssetEnumerator` (walkdir + `globset` excludes, default exclude set);
`ConfigResolver` (sidecar → nearest manifest → uncovered); `auditah.toml` config
parse; `toml_edit` read preserving comments. Verify: `temptree`-based
integration tests for resolution precedence (tests 3, 4, 5, 13).

### Phase 4 — `audit` Command
`run_audit` orchestration using discovery + resolution; coverage/resolution/
obligation checks producing `Finding`s; grouped report; exit code. Verify:
integration tests covering tests 1, 2, 6, 7, 8, 9, 10, 11, 12.

### Phase 5 — `credits` Command
Group-by-author renderer, CC0 suppression, modification-notice emission,
`CREDITS.md` write via `toml_edit`-safe path. Verify: integration tests 14, 15.

### Phase 6 — `add` / `init-pack`
`add` scaffolds a sidecar (interactive prompts); `init-pack` writes a folder
`manifest.toml`. Verify: command produces parseable files that pass `audit`.

### Phase 7 — Wire-up (was "Migrate + Wire-up")
~~Convert the 3 existing assets to new format; drop `reuse` artifacts;~~
~~migration test against real source files (read-only);~~ DROPPED: no real
assets belong in this standalone tool's plan. Retained: finalize README +
justfile targets (`audit`, `credits`, `lint`). Verify: `audit` clean on a
synthetic temptree tree (covered by integration tests in earlier phases).

### Phase 8 — Verification
Walk each acceptance criterion (1–7 above) as a discrete task; confirm pass.
