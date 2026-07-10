# Well-Known SPDX Licenses (embedded zip) + default-fail grid correction

## Problem

After the `LicenseRef`-collapse refactor, every license requires hand-authoring — even universal ones like MIT. Worse, the refactor regressed a critical contract: `add-license Foo` now writes a **permissive** grid (`LicenseTerms::permissive()` → `manual_review = false`, `allows_commercial_use = true`, `allows_redistribution = true`), so a user can scaffold a custom license that passes audit with all permissions granted and never engage with the grid. That violates the project's "fail by default" rule: a scaffolded-but-unfilled license must FAIL audit until a human reviews and acknowledges it.

Separately, there is no way to obtain canonical license text without manual hunting.

## Solution

1. **Embed the full SPDX text corpus** (814 `.txt` files, 1.89MB zipped, measured) as a `build.rs`-generated zip blob, accessed via the `zip` crate's `ZipArchive::by_name()` for single-file extraction. Build input is the already-vendored `well_known_licenses/` directory.
2. **`add-license` becomes dual-path with explicit dispatch:**
   - `add-license <name>` (no flag) → well-known path: case-insensitive complete-string match against a normalized index built from the zip's text entries; if exactly one match, extract canonical text + (authored grid OR a default-fail placeholder grid + printed warning); if zero or ambiguous → error.
   - `add-license --custom <name>` → `LicenseRef-<name>` path: write a default-fail template grid + printed warning. `--custom` on a name that matches a well-known id errors.
3. **Introduce a single `LicenseTerms::default_fail()`** (maximally restrictive: `manual_review = true`, all permissions false, `derivatives = "disallowed"`), used in **both** placeholder scenarios — replacing the permissive template the refactor introduced. This corrects the regression.
4. **Seed `LicenseRegistry::load`** with authored well-known grids parsed from the embedded zip *before* `merge_project_local`, so a hand-authored `LICENSES/<id>.toml` still overrides.
5. **Drop the dead `spdx` crate** (zero references) and add `zip`.

## Acceptance Criteria

- `add-license MIT` extracts canonical `LICENSES/MIT.txt` + the authored MIT grid; no terminal warning; `manual_review = false`.
- `add-license mit` (case-insensitive) resolves to the same canonical `MIT` (canonical casing on disk).
- `add-license Bzip2-1.0.6` (text only, no authored grid) extracts `.txt` + writes a `default_fail()` grid + **prints a warning** that the grid must be filled in.
- `add-license --custom Foo` writes `LICENSES/LicenseRef-Foo.toml` with the `default_fail()` grid + prints a warning.
- `add-license --custom MIT` errors ("use `add-license MIT` for well-known licenses").
- `add-license NotReal` (no flag, zero matches) errors ("unknown SPDX id, use `--custom` for a custom license").
- `add-license Foo` where `Foo` matches multiple entries case-insensitively → errors (ambiguous).
- Matching is **complete-string only**: `add-license M` does NOT match `MIT` (no partial/substring matching).
- `LicenseRegistry::load` resolves authored well-known SPDX ids (MIT, etc.) from the embedded zip, with no `LICENSES/*.toml` present.
- A hand-authored `LICENSES/MIT.toml` overrides the embedded well-known MIT grid.
- A `default_fail()` grid FAILs audit until its id is in `manual_review_acknowledged` (existing `ManualReviewRequired` behavior, unchanged).
- `spdx` crate removed; `zip` crate added; binary embeds `spdx-licenses.zip` via `include_bytes!`.
- `cargo build --tests`, `cargo clippy --tests` (with `unwrap_used`/`expect_used`), `cargo fmt --check`, and the full suite are clean.

## Phases

### Phase 1 — Data + build infra
Add `zip` to `[dependencies]`, remove `spdx`. Create `build.rs` that, at build time, zips `well_known_licenses/*.{txt,toml}` into `spdx-licenses.zip` at the crate root (only the `.txt` files exist today; `.toml` grids are added in Phase 2). The artifact is already gitignored. `build.rs` uses the `zip` crate as a build-dependency. Create `src/well_known.rs` with `const SPDX_ZIP: &[u8] = include_bytes!("../spdx-licenses.zip");` and a lazily-built `ZipArchive` wrapper.

### Phase 2 — Author the ~15 starter grids
Author `.toml` grids in `well_known_licenses/` for: MIT, ISC, BSD-2-Clause, BSD-3-Clause, 0BSD, Apache-2.0, CC0-1.0, CC-BY-4.0, CC-BY-SA-4.0, CC-BY-ND-4.0, OFL-1.1, GPL-3.0-only, LGPL-3.0-only, MPL-2.0. Each grid is the SPDX id, a human name, canonical url, and a correct `LicenseTerms` mapping. (These are authored by hand per the dialectic — the grid is auditah's contribution.)

### Phase 3 — `default_fail()` + replace the template default
Add `LicenseTerms::default_fail()`. Change `render_license_template` to use it (not `permissive()`). Update the header comment text to reflect that the grid FAILs audit until reviewed + acknowledged (not "permissive baseline"). Correct existing `--custom`-path tests that assert permissive defaults. Add the printed warning on placeholder paths (no-authored-grid well-known + `--custom`).

### Phase 4 — `well_known` module: index, match, extract
Build a normalized case-insensitive index from the zip's text entries (operator's discretion on lower- vs upper-case; lower is recommended). Index maps `normalized_name → real_canonical_name`. Expose: `fn text_ids()` (iteration, for any listing/debugging), `fn resolve(name) -> ResolveResult` (`Found(canonical)`, `Ambiguous(Vec)`, `NotFound`), `fn grid_for(canonical) -> Option<String>` (authored `.toml` from zip or `None`), `fn extract_text(canonical) -> String`, `fn extract_grid(canonical) -> Option<String>`.

### Phase 5 — `add-license` dual dispatch
Add `--custom` flag to `AddLicenseCmd`. Branch in `run()`:
- `--custom` set: if `name` case-insensitively matches a well-known id → error. Else → existing `write_license_template` path (now using `default_fail()`), print warning.
- `--custom` not set: `well_known::resolve(name)` → `Found` → extract text + (authored grid or default-fail placeholder + warning); `Ambiguous` → error listing candidates; `NotFound` → error suggesting `--custom`.

### Phase 6 — `LicenseRegistry::load` seeding
Parse authored grids from the embedded zip into the empty `entries` map **before** `merge_project_local`. Only entries with a `.toml` in the zip seed in (the ~15 authored). A hand-authored `LICENSES/<id>.toml` still wins because `merge_project_local` runs after and inserts by id.

### Phase 7 — Tests + cleanup
Test cases from the table below. Verify `spdx` is gone from `Cargo.lock`. Ensure lints/fmt/suite clean.

---

## Dialectical Outcomes (Why)

- **Embed vs sidecar vs runtime-fetch.** Runtime fetch was rejected (no HTTP dep, network at audit/CI time, cache/offline complexity). Sidecar zip was considered, but `include_bytes!` of the zip was chosen: 1.89MB on a ~6MiB release binary is an acceptable ~31% bump, gives offline operation, and is one file to ship. The corpus is source-controlled raw in `well_known_licenses/`; the zip is a `build.rs` build artifact.
- **`LicenseRef-` vs SPDX id coexistence.** The prior `LicenseRef-`-collapse was driven by the wart of dual id forms with different validation paths. Well-known licenses reintroduce SPDX ids, but they flow through the **same** registry + same audit pipeline (no special-casing). `LicenseRef-*` remains the only form for *custom* licenses. The distinction is authoring-provenance, not a runtime category.
- **Explicit `--custom` dispatch over magic fallthrough.** Originally considered "auto-detect well-known, else fall through to custom." Rejected: fallthrough creates ambiguity and error-class confusion. `--custom` makes intent explicit; without it, the command always sources from the zip and errors cleanly on no-match.
- **Case-insensitive complete-string matching with ambiguity detection.** `.contains` is insufficient (could match many). Build a normalized index; require exactly one match. `M` must NOT match `MIT`.
- **`default_fail()` is maximally restrictive.** Not just `manual_review = true`, but all permissions false + `derivatives = "disallowed"`. Reason: if a user acknowledges without reading, the grid still grants nothing. The whole point is "a scaffolded-but-unfilled license must not pass."
- **Single default-fail grid for both placeholder scenarios.** Unifies the "ungridded well-known text" and "new custom license" cases under one template. Keeps behavior consistent and avoids two near-identical scaffolds.
- **Copy text even without a grid.** Rather than rejecting `add-license Bzip2-1.0.6` for lack of an authored grid, copy the canonical `.txt` and write a default-fail placeholder grid + warning. The license text is the hard-to-source part; the grid is the user's job once the text is in place.
- **Audit contract unchanged.** `check_license_text` (`audit.rs`) is id-form-agnostic and checks `LICENSES/<id>.txt` on disk. The well-known feature changes *where text comes from*, not *how it's checked*. No special-casing needed.
- **`spdx` crate dropped.** Zero references in `src/` or `tests/`; we match against our own grid set, not the SPDX master list. The crate exists to parse composite expressions / validate identifiers, neither of which auditah does.
- **Rejected: `incomplete: bool` field.** A dedicated "incomplete grid" field would reopen the `LicenseTerms` schema for a "kind of crappy" fallback. `manual_review = true` already means "audit fails until a human engages" — exactly the placeholder's job. Reuse it; don't add a concept.

## Relevant Files (Where)

**Created:**
- `build.rs` — zips `well_known_licenses/*` → `spdx-licenses.zip` at build time.
- `src/well_known.rs` — embedded zip access, normalized index, match, extract.
- `well_known_licenses/<id>.toml` — ~15 authored grids (Phase 2).

**Modified:**
- `Cargo.toml` — add `zip` (dep) + `zip` (build-dep); remove `spdx`.
- `src/model/terms.rs` — add `LicenseTerms::default_fail()`.
- `src/add_license.rs` — `render_license_template` uses `default_fail()`; header/term comments updated; exported helpers for the well-known path (extract text, extract/write grid). Printed warning helper.
- `src/cli/add_license_cmd.rs` — add `--custom` flag; dual-dispatch in `run()`.
- `src/registry.rs` — `LicenseRegistry::load` seeds from embedded authored grids before `merge_project_local`.
- `src/lib.rs` — declare `pub mod well_known;`.

**Unchanged but load-bearing:**
- `src/audit.rs::check_license_text` — stays id-form-agnostic.
- `src/model/terms.rs::Derivatives` — `disallowed`/`allowed`/`share-alike`.

## Key Code Context (What)

Current `LicenseTerms` + `permissive()` (the latter is what the regression used; `default_fail()` is the corrective addition):
```rust
pub struct LicenseTerms {
    pub requires_attribution: bool,
    pub requires_license_notice: bool,
    pub requires_source_disclosure: bool,
    pub derivatives: Derivatives,           // Disallowed | Allowed | ShareAlike
    pub requires_modification_notice: bool,
    pub allows_commercial_use: bool,
    pub allows_redistribution: bool,
    pub manual_review: bool,
}

pub fn permissive() -> Self {
    Self {
        requires_attribution: false,
        requires_license_notice: false,
        requires_source_disclosure: false,
        derivatives: Derivatives::Allowed,
        requires_modification_notice: false,
        allows_commercial_use: true,
        allows_redistribution: true,
        manual_review: false,
    }
}
```

`default_fail()` target shape:
```rust
pub fn default_fail() -> Self {
    Self {
        requires_attribution: false,
        requires_license_notice: false,
        requires_source_disclosure: false,
        derivatives: Derivatives::Disallowed,
        requires_modification_notice: false,
        allows_commercial_use: false,
        allows_redistribution: false,
        manual_review: true,
    }
}
```

Current `add-license` dispatch (single path, `LicenseRef-` always):
```rust
// src/cli/add_license_cmd.rs
pub struct AddLicenseCmd {
    pub name: String,
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

// src/add_license.rs
pub fn license_ref_id(name: &str) -> String {
    if name.starts_with("LicenseRef-") { name.to_string() } else { format!("LicenseRef-{name}") }
}
pub fn render_license_template(id: &str) -> String {
    let terms = LicenseTerms::permissive();   // ← the regression; becomes default_fail()
    // ...
}
pub fn write_license_template(services, root, name) -> Result<PathBuf, Report<AddLicenseError>>
```

Current registry load (empty map → merge project-local). Seeding inserts authored grids between these two steps:
```rust
pub fn load(fs: &FsService, project_root: &Path) -> Result<Self, Report<RegistryError>> {
    let mut entries = HashMap::new();
    // ← Phase 6: seed authored well-known grids from embedded zip here
    merge_project_local(fs, project_root, &mut entries)?;
    Ok(Self { entries })
}
```

`FsService` write API (used to materialize text + grid):
```rust
pub fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>>
pub fn exists(&self, path: &Path) -> bool
```

## Implementation Algorithm (How)

### `build.rs`
1. Glob `well_known_licenses/*` (both `.txt` and `.toml`).
2. Open `spdx-licenses.zip` for write via the `zip` crate.
3. For each file, add a zip entry whose **name is the bare filename** (e.g. `MIT.txt`, `MIT.toml`) — flat, no directory prefix.
4. Emit `cargo:rerun-if-changed=well_known_licenses`.

### `well_known::resolve(name) -> ResolveResult`
1. Build (lazily, `once_cell::sync::Lazy` or `std::sync::OnceLock`) a `HashMap<String, String>` from the zip: for each entry ending in `.txt`, key = `filename_without_ext.to_lowercase()`, value = canonical filename_without_ext. (`.toml` entries are grids, not text-ids; the index is over text ids. A text id with no `.toml` is the ungridded case.)
2. Look up `name.to_lowercase()` in the index. Because the index is keyed by normalized name and maps to exactly one canonical id, "ambiguity" in the SPDX corpus is already impossible (ids are unique case-insensitively by SPDX spec). **Ambiguity detection** still exists in the API surface but, for the SPDX corpus, resolves to a single canonical id. Keep the `ResolveResult::Ambiguous` variant for forward-compat and clarity.
3. Return `Found(canonical)`, or `NotFound`.

> Correction noted: a `HashMap` from normalized→canonical cannot itself surface multiple candidates (keys are unique). True ambiguity detection would require an index that can yield >1 match per normalized key. For the SPDX corpus (case-insensitively unique ids) this is moot, but keep `ResolveResult` with `Found`/`NotFound` and drop `Ambiguous` **unless** the implementer wants a `Vec`-backed index for forward-compat. The acceptance criterion "ambiguous match errors" is preserved by: if the implementer builds the index as `HashMap<String, Vec<String>>` (normalized→all canonicals), a `Vec` of length >1 → `Ambiguous`. Recommended: build the `Vec`-backed index to satisfy the AC literally and keep the door open for near-duplicate ids.

### `add-license` dispatch (`run`)
```
if cmd.custom:
    if well_known::resolve(cmd.name).is_found():
        error "use `add-license <name>` for well-known licenses"
    id = license_ref_id(cmd.name)
    write_license_template(...)  // default_fail() grid
    print warning (custom license must be reviewed)
else:
    match well_known::resolve(cmd.name):
        Found(canonical):
            extract_text(canonical) -> write LICENSES/<canonical>.txt
            match well_known::grid_for(canonical):
                Some(grid_toml) -> write LICENSES/<canonical>.toml  // authored
                None -> write_license_placeholder(canonical)        // default_fail() + warning
        Ambiguous(candidates) -> error listing candidates
        NotFound -> error "unknown SPDX id; use `--custom` for a custom license"
```

### Registry seeding (Phase 6)
For each `.toml` entry in the embedded zip: parse as `LicenseRegistryEntry`, insert into `entries` by id, *before* `merge_project_local`.

## Anti-Goals (Out of Scope)

- Runtime network fetch of license texts.
- Authoring all 814 grids upfront (incremental; ~15 now).
- Any audit-side special-casing for well-known vs custom ids.
- Re-introducing a `text` field on `LicenseRegistryEntry`.
- Re-introducing dual-form id resolution with different validation paths.
- A new `incomplete` field on `LicenseTerms`.
- `--force` to overwrite an existing `LICENSES/<id>.toml`.
- Full SPDX-expression parsing (`A OR B`, `WITH exception`) — out of scope; auditah matches single ids.

## Edge Cases & Gotchas

- **The regression being corrected:** today `add-license Foo` writes `permissive()` → a custom license that passes audit with all rights granted. Every `--custom` test currently asserting permissive defaults must be updated to assert `default_fail()`.
- **Canonical casing on disk.** `add-license mit` must write `LICENSES/MIT.txt`, not `mit.txt`. The index maps normalized→canonical; always write using the canonical from the index.
- **`build.rs` must be deterministic & rerun-safe.** Overwrite `spdx-licenses.zip` each build; emit `rerun-if-changed`. The artifact is gitignored already.
- **Flat zip layout.** Entries must be bare filenames (`MIT.txt`), not `well_known_licenses/MIT.txt` — `ZipArchive::by_name("MIT.txt")` is the access path.
- **`build.rs` and the `zip` crate.** `zip` must be a build-dependency too (or a standalone packaging step). Decide: `[build-dependencies] zip = ...`. Alternatively a `just`/`xtask` target generates the zip and `build.rs` only `include_bytes!`s it if present — but a pure `build.rs` is simpler and always-fresh.
- **Phase 6 seeding changes observable registry behavior.** After seeding, `LicenseRegistry::load` resolves authored well-known ids (MIT, etc.) even when `LICENSES/` is empty. Existing tests that construct a registry expecting `get("MIT").is_none()` or assert a specific `len()` on an empty project will break. The shared test fixture (the registry builder / `Fixture`-style helpers in `tests/common/mod.rs`) constructs registries in-memory via `LicenseRegistryBuilder`, which does NOT seed — so those tests are unaffected. Only tests calling `LicenseRegistry::load` directly need review. Audit these explicitly in Phase 6.
- **SPDX id casing is significant for filename writes but insignificant for matching.** The index normalizes; the write path must restore canonical casing.
- **`add-license --custom` on a name with no match must still work** (it never consults the zip for matching — only to *reject* if the name happens to be a known id). Keep the reject-check case-insensitive too, so `--custom mit` errors just like `--custom MIT`.
- **`LicenseTerms` derives needed.** Authored `.toml` grids in `well_known_licenses/` must deserialize via the existing serde setup on `LicenseTerms`/`LicenseRegistryEntry` — no schema change, just correct field values.
- **`--custom` idempotency on `LicenseRef-` prefix.** `license_ref_id` is already idempotent; `--custom LicenseRef-Foo` still yields `LicenseRef-Foo`. Keep this.
- **No partial matching.** `M` ≠ `MIT`. Complete-string only.

## Navigation Anchors

- `src/cli/add_license_cmd.rs::run` — primary CLI dispatch; the dual-path branch lives here.
- `src/add_license.rs::write_license_template` + `render_license_template` — the `--custom` scaffold path; `default_fail()` plugs in here.
- `src/well_known.rs` (new) — all zip/index/match/extract logic; entry points `resolve`, `grid_for`, `extract_text`, `extract_grid`.
- `src/registry.rs::LicenseRegistry::load` — the seeding insertion point.
- `build.rs` (new) — the zip generation.
- `src/model/terms.rs::LicenseTerms::default_fail` — the corrective constructor.

## Dependency Mappings

- **Add `zip`** (crates.io) — both `[dependencies]` (runtime extraction) and `[build-dependencies]` (zip generation in `build.rs`). Pin a current `0.x`.
- **Add `once_cell`** OR use `std::sync::OnceLock` (stable) for the lazy index — prefer `OnceLock` to avoid a new dep.
- **Remove `spdx`** — dead (zero references).
- No other new deps.

## Test Strategies

- **Phase 1 (build):** `cargo build` succeeds with the zip embedded; a unit test in `well_known.rs` reads `SPDX_ZIP` via `ZipArchive`, asserts entry count ≈ 814 and that `by_name("MIT.txt")` returns non-empty content.
- **Phase 2 (grids):** a test loads each authored grid from the zip and round-trips it through `toml::from_str::<LicenseRegistryEntry>` (parses, expected id). Parameterize via rstest over the 14 ids.
- **Phase 3 (default_fail):** unit test on `LicenseTerms::default_fail()` asserting all flags per spec. Update existing `add-license` template tests to expect `default_fail()` shape. A test asserting the placeholder template's comment text mentions the review/acknowledge flow.
- **Phase 4 (match):** unit tests on `resolve`: `mit`→`Found("MIT")`; `M`→`NotFound`; `NotReal`→`NotFound`; a synthetic ambiguity (construct an index with two same-normalized keys)→`Ambiguous`.
- **Phase 5 (dispatch):** integration tests in `tests/` (temp dir + real `FsService`):
  - `add-license MIT` writes `LICENSES/MIT.txt` + authored grid; no warning captured (or warning absent).
  - `add-license Bzip2-1.0.6` writes `.txt` + `default_fail()` grid; warning present.
  - `add-license --custom Foo` writes `LicenseRef-Foo.toml` `default_fail()`; warning present.
  - `add-license --custom MIT` → Err.
  - `add-license NotReal` → Err.
- **Phase 6 (seeding):** `LicenseRegistry::load` with an empty `LICENSES/` dir resolves `MIT` (seeded). With a hand-authored `LICENSES/MIT.toml`, the authored grid's `terms` win over the seeded one.
- **Phase 7 (cleanup):** `grep -r 'spdx::' src/ tests/` is empty; `cargo build --tests`, `cargo clippy --tests` (with the restriction lints), `cargo fmt --check`, and `cargo nextest run` all clean.

## Test Cases

| # | Case | Expected |
|---|---|---|
| 1 | `add-license MIT` | extracts `LICENSES/MIT.txt` + authored grid; `manual_review=false`; no warning |
| 2 | `add-license mit` | case-insensitive → same as #1; canonical `MIT` casing on disk |
| 3 | `add-license Bzip2-1.0.6` (no grid) | extracts `.txt`; writes `default_fail()` grid; prints warning |
| 4 | `add-license --custom Foo` | writes `LicenseRef-Foo.toml` `default_fail()` grid; prints warning |
| 5 | `add-license --custom MIT` | errors (known id, must use non-custom path) |
| 6 | `add-license NotReal` (no flag) | errors (zero matches) |
| 7 | `add-license Foo` (synthetic ambiguous index) | errors (ambiguous), lists candidates |
| 8 | `LicenseTerms::default_fail()` | `manual_review=true`, all `allows_*`=false, `requires_*`=false, `derivatives=disallowed` |
| 9 | `default_fail()` grid FAILs audit until acknowledged | FAIL `ManualReviewRequired` → ack → clean |
| 10 | `LicenseRegistry::load` with empty `LICENSES/` | resolves authored well-known ids (MIT etc.) |
| 11 | `LICENSES/MIT.toml` present | authored grid overrides embedded well-known MIT grid |
| 12 | embedded zip `include_bytes!` | `by_name("MIT.txt")` returns non-empty; entry count ≈814 |
| 13 | `spdx` crate removed, `zip` added | `Cargo.lock` reflects; `cargo build` clean |
| 14 | matching is complete-string only | `add-license M` does NOT match `MIT` |
| 15 | `--custom` template correctness | every `[terms]` field commented; placeholder explains fill-in + acknowledge flow |
