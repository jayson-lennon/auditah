# `auditah bom` — license bill of materials

## Problem

After removing the FLAG severity, three obligations (license notice, source
disclosure, share-alike) produce no audit finding — by design. But they still
need to be **visible and actionable** somewhere, or they vanish from the
workflow entirely. There is no artifact that answers "what obligations does
this distribution carry?" or surfaces "must provide source for X" as a human
TODO. `credits` answers attribution; nothing answers compliance obligations.

## Solution

Add an `auditah bom` command (mirroring `credits`) that produces a `BOM.md`
file from the same discovery/resolution pipeline audit and credits already
walk. The BOM has two sections: **a per-license summary** (distinct licenses
in use, their obligation/permission flags, asset counts) and **action items**
(the uncheckable obligations — source disclosure, license notice, share-alike
— surfaced as TODOs, plus a compatibility warning if multiple distinct
share-alike licenses are in use).

## Acceptance Criteria

1. `auditah bom` writes `<root>/BOM.md` (overridable via `--output`); exit 0
   on success.
2. The per-license summary lists every distinct license id in use with its
   effective obligation/permission flags and asset count.
3. CC0/MIT/permissive assets appear in the summary even though they don't need
   attribution — because they may still carry notice obligations.
4. The action-items section lists `requires_source_disclosure` licenses with
   their asset paths, `requires_license_notice` licenses (pointing at
   NOTICES.md), and `share-alike` licenses with the relicense obligation.
5. If >1 distinct share-alike license id is in use, an action item warns of
   potential conflict.
6. An all-permissive project (CC0/MIT only, no notice/source/SA obligations)
   produces a summary but an empty/no-action-items section.
7. `cargo build --tests`, `cargo clippy --tests`, `cargo fmt --check`, full
   suite clean.
8. README is **not** updated in this task (separate follow-up).
9. `BOM.md` is excluded from asset enumeration (like `CREDITS.md`) so
   re-running `audit` after `bom` does not treat the BOM as an asset.

## Phases

### Phase 1: CLI wiring

Wire the new `bom` subcommand into the CLI, mirroring `credits_cmd.rs`
exactly.

- New `src/cli/bom_cmd.rs`: a `BomCmd` struct with `--root` (default `.`) and
  `--output` (optional; defaults to `<root>/BOM.md`). A `run()` function that
  constructs `Services::real(root)`, loads `Config`, builds a `BomCtx`, calls
  `generate_bom(&ctx, &output)`, and prints `bom: wrote {path}`. Returns
  `Ok(CommandStatus::Success)`.
- `src/main.rs`: add `Command::Bom(BomCmd)` to the `Command` enum and the
  dispatch match arm.
- `src/discovery.rs`: add `"BOM.md"` to `DEFAULT_EXCLUDES` (next to
  `CREDITS.md`) so the generated BOM is not treated as an asset.

### Phase 2: Collection (`src/bom.rs`)

The data-gathering layer. Walks the same pipeline as `credits`, but collects
**all** licenses in use (not just attribution-required ones), and groups by
license id.

- `BomError` (colocated with `BomCtx`).
- `BomCtx<'a>` — mirrors `CreditsCtx`: `{ services, config, root }`.
- `LicenseSummary` — per-license aggregate: `{ entry metadata (id, name,
  url), effective terms, asset paths }`.
- `collect_bom(ctx) -> Result<Vec<LicenseSummary>, Report<BomError>>`:
  - Build excludes (`all_excludes` + `ExcludeMatcher::new`), same defense-
    in-depth pattern as `build_excludes` in `credits.rs`.
  - `enumerate` assets.
  - For each asset: `resolve`; skip uncovered (`source == None` or
    `record == None`); `registry.get(record.license)`; skip if unknown
    (audit catches that); `effective_terms(entry.terms, record.overrides)`.
  - Group by license id into a `BTreeMap<String, LicenseSummary>` (sorted for
    stable output). Append each asset path to the summary's asset list.
- `build_excludes` helper (same shape as `credits.rs::build_excludes` but
  returning `BomError`).

### Phase 3: Rendering

Pure functions that turn `Vec<LicenseSummary>` into a Markdown string. Two
sections.

- `render_bom(&[LicenseSummary]) -> String`:
  - Header: `# License Bill of Materials\n\n`.
  - `## Licenses in use` section: for each `LicenseSummary`, render the
    license name + id + asset count, then a bullet list of the
    obligation/permission flags that are set (e.g. "Commercial use:
    permitted", "Derivatives: allowed", "Attribution: required",
    "License notice: **MUST reproduce**", "Source disclosure: **MUST offer
    corresponding source**", "Share-alike: modified assets must ship under
    this license"). Surface the uncheckable obligations (notice/source/SA) in
    bold as they are the action items.
  - `## Action items` section: call `derive_action_items(&summaries)`. If
    empty, render a short note like "_No outstanding compliance actions._".
- `derive_action_items(&[LicenseSummary]) -> Vec<String>`:
  - For each license with `requires_source_disclosure`: "Offer
    corresponding source for {n} {id} asset(s): {paths}".
  - For each license with `requires_license_notice`: "Reproduce license text
    for {id} in your distribution — see NOTICES.md".
  - For each license with `derivatives == ShareAlike`: "Share-alike: modified
    {id} assets must ship under {id}".
  - If >1 distinct `ShareAlike` license id: prepend a warning "Multiple
    share-alike licenses in use ({ids}) — verify derivative works can
    satisfy both."
- `generate_bom(ctx, output_path)`: orchestrate `collect_bom` →
  `render_bom` → `fs.write`. Same shape as `generate_credits`.
- `default_output_path(root) -> PathBuf`: `root.join("BOM.md")`.

### Phase 4: Tests

Integration tests in `tests/bom_pipeline.rs` mirroring the structure of
`tests/audit_pipeline.rs` / `tests/credits_pipeline.rs`. One BDD test per
test case in the plan's table. Uses the shared `common/mod.rs` helpers
(`services_with`, `LicenseSpec`, `permissive_terms`, etc.).

### Phase 5: Verification

Walk each acceptance criterion and confirm it holds.

## Dialectical Outcomes (Why)

1. **File output, not stdout.** The BOM is a compliance artifact that should
   live alongside `CREDITS.md` / `NOTICES.md` — persisted, checkable,
   shippable. Matches the `credits` command's established pattern. Rejected
   stdout-only (no persistence) and both (extra CLI surface for marginal
   value).

2. **Two-section structure (summary + action items).** The summary answers
   "what am I shipping under?"; the action-items section answers "what must I
   do?". The action-items section is the BOM's reason to exist — without it,
   the reader has to scan every license to find obligations. Rejected
   summary-only (obligations hidden in detail) and per-asset-flat (50 MIT
   rows is noise).

3. **Surface multi-share-alike conflict.** If two distinct `share-alike`
   licenses are in use, modified derivatives of assets under each cannot
   satisfy both relicense demands simultaneously. This is the one
   checkable compatibility concern and the user explicitly asked for
   "compatible with the other license grids." Rejected pure fact-listing
   (human has to detect the conflict themselves).

4. **Collect ALL licenses (including permissive).** Unlike `credits` (which
   filters to attribution-required), the BOM collects every license in use —
   because even MIT/CC0 carry a notice obligation. Filtering to attribution-
   required would hide notice obligations on permissive licenses.

5. **README fix deferred.** The README is stale (mentions `init-licenses`,
   lowercase `licenses/`, FLAG severity, inline `text` field) but fixing it
   is mechanical churn independent of the BOM. Separate task to keep this
   focused.

6. **`BOM.md` added to DEFAULT_EXCLUDES.** Discovered during tracing:
   `CREDITS.md` is excluded but `BOM.md` is not. Without this, running
   `audit` after `bom` would treat `BOM.md` as an unlicensed asset → FAIL.
   Same fix `CREDITS.md` already has.

## Relevant Files (Where)

### New files

- `src/bom.rs` — the BOM subsystem (collection + rendering + orchestration).
- `src/cli/bom_cmd.rs` — the CLI command struct + `run()` handler.
- `tests/bom_pipeline.rs` — integration tests.

### Modified files

- `src/main.rs` — add `Command::Bom(BomCmd)` to enum + dispatch.
- `src/lib.rs` — add `pub mod bom;`.
- `src/cli/mod.rs` — add `pub mod bom_cmd;` (if there's a module declaration
  there; verify at implementation time).
- `src/discovery.rs` — add `"BOM.md"` to `DEFAULT_EXCLUDES`.

## Key Code Context (What)

### `CreditsCmd` — the template for `BomCmd`

```rust
// src/cli/credits_cmd.rs
#[derive(Debug, Args)]
pub struct CreditsCmd {
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub fn run(cmd: &CreditsCmd) -> Result<CommandStatus, Report<AppError>> {
    let root = &cmd.root;
    let services = Services::real(root).change_context(AppError)?;
    let config = Config::load(&services.fs, root)
        .change_context(AppError)
        .attach("failed to load config")?;
    let output = cmd.output.clone()
        .unwrap_or_else(|| default_output_path(root));
    let ctx = CreditsCtx { services: &services, config: &config, root };
    generate_credits(&ctx, &output)
        .change_context(AppError)
        .attach("failed to generate credits")?;
    println!("credits: wrote {}", output.display());
    Ok(CommandStatus::Success)
}
```

### `CreditsCtx` — the template for `BomCtx`

```rust
// src/credits.rs
pub struct CreditsCtx<'a> {
    pub services: &'a Services,
    pub config: &'a Config,
    pub root: &'a Path,
}
```

### `collect_credits` — the template for `collect_bom`

Key difference: `collect_bom` does NOT filter to `requires_attribution`. It
collects all resolved assets and groups by license id.

```rust
// src/credits.rs (abbreviated)
pub(crate) fn collect_credits(ctx: &CreditsCtx)
    -> Result<BTreeMap<String, Vec<CreditEntry>>, Report<CreditsError>>
{
    let excludes = build_excludes(ctx)?;
    let assets = enumerate(&ctx.services.fs, ctx.root, &excludes)...;
    let mut by_author: BTreeMap<String, Vec<CreditEntry>> = BTreeMap::new();
    for asset in &assets {
        let Some(record) = resolve(&ctx.services.fs, asset, ctx.root)...?.record else { continue; };
        if let Some(entry) = entry_if_attribution_required(&record, ctx) {
            by_author.entry(record.author.clone()).or_default().push(entry);
        }
    }
    sort_entries(&mut by_author);
    Ok(by_author)
}
```

### `build_excludes` defense-in-depth pattern

```rust
// src/credits.rs
fn build_excludes(ctx: &CreditsCtx) -> Result<ExcludeMatcher, Report<CreditsError>> {
    let patterns = crate::discovery::all_excludes(&ctx.config.exclude);
    ExcludeMatcher::new(&patterns)
        .change_context(CreditsError)
        .attach("invalid exclude glob in auditah.toml")
}
```

### `LicenseTerms` — the obligation/permission model

```rust
// src/model/terms.rs
pub struct LicenseTerms {
    pub requires_attribution: bool,
    pub requires_license_notice: bool,
    pub requires_source_disclosure: bool,
    pub derivatives: Derivatives,              // Disallowed | Allowed | ShareAlike
    pub requires_modification_notice: bool,
    pub allows_commercial_use: bool,
    pub allows_redistribution: bool,
    pub manual_review: bool,                   // license-only, not in Overrides
}
```

The action items are derived from: `requires_source_disclosure`,
`requires_license_notice`, and `derivatives == Derivatives::ShareAlike`.

### `LicenseRegistryEntry` — license metadata

```rust
// src/model/license.rs
pub struct LicenseRegistryEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    pub terms: LicenseTerms,
    pub notes: Option<String>,
}
```

### `AttributionRecord` — per-asset attribution

```rust
// src/model/attribution.rs
pub struct AttributionRecord {
    pub title: String,
    pub author: String,
    pub year: u16,
    pub license: String,
    pub source: String,
    pub modified: bool,
    pub package: Option<String>,
    pub overrides: Overrides,
}
```

### `DEFAULT_EXCLUDES` — where to add `BOM.md`

```rust
// src/discovery.rs
pub const DEFAULT_EXCLUDES: &[&str] = &[
    // ...
    "CREDITS.md",   // <- add "BOM.md" right after this
    // ...
];
```

## Implementation Algorithm (How)

### `collect_bom`

```
build_excludes(ctx) -> ExcludeMatcher
enumerate(fs, root, excludes) -> Vec<PathBuf>
for each asset:
    resolve(fs, asset, root) -> ResolvedAsset
    if source == None OR record == None: skip (uncovered — audit handles)
    registry.get(record.license) -> Option<&LicenseRegistryEntry>
    if None: skip (unknown license — audit handles)
    effective_terms(entry.terms, record.overrides) -> LicenseTerms
    group by entry.id: BTreeMap<String, LicenseSummary>
        .entry(id).or_insert(LicenseSummary { id, name, url, terms, assets: [] })
        .assets.push(asset path)
return Vec<LicenseSummary> (sorted by id via BTreeMap)
```

Note: `effective_terms` is computed per-asset. If two assets under the same
license have different `[overrides]`, their effective terms differ. The
summary should use the **base license terms** (from the registry entry) for
the per-license flags, since overrides are per-asset and don't change the
license's inherent obligations. Asset-level overrides affect audit/credits,
not the BOM's license summary. **Gotcha:** this means the BOM summary shows
the license's declared terms, not the merged per-asset terms — which is the
correct semantic (the BOM describes the license, not each asset's override).

### `derive_action_items`

```
action_items = []

// Share-alike conflict check (first, as a warning)
sa_licenses = summaries.filter(|s| s.terms.derivatives == ShareAlike)
if sa_licenses.len() > 1:
    ids = sa_licenses.map(|s| s.id).join(", ")
    action_items.push("⚠ Multiple share-alike licenses in use ({ids}) — verify derivative works can satisfy both.")

// Per-license action items
for summary in summaries (sorted):
    if summary.terms.requires_source_disclosure:
        paths = summary.assets.join(", ")
        action_items.push("Offer corresponding source for {summary.assets.len()} {summary.id} asset(s): {paths}")
    if summary.terms.requires_license_notice:
        action_items.push("Reproduce license text for {summary.id} in your distribution — see NOTICES.md")
    if summary.terms.derivatives == ShareAlike:
        action_items.push("Share-alike: modified {summary.id} assets must ship under {summary.id}")

return action_items
```

### `render_bom`

```
out = "# License Bill of Materials\n\n"

if summaries.is_empty():
    out += "_No licensed assets found._\n"
    return out

out += "## Licenses in use\n\n"
for summary in summaries:
    out += "### {summary.name} ({summary.id}) — {summary.assets.len()} asset(s)\n\n"
    out += render_terms_bullets(&summary.terms)
    out += "\n"

action_items = derive_action_items(summaries)
out += "## Action items\n\n"
if action_items.is_empty():
    out += "_No outstanding compliance actions._\n"
else:
    for (i, item) in action_items.enumerate():
        out += "{i+1}. {item}\n"

return out
```

## Anti-Goals (Out of Scope)

1. **README update.** The README is stale but fixing it is a separate task.
2. **NOTICES.md generation.** The BOM points at NOTICES.md but does not
   generate it. NOTICES generation is a separate `credits` enhancement
   (agreed in the prior dialectic but not yet implemented).
3. **Per-asset override rendering in the BOM.** The BOM summarizes per-
   license; per-asset overrides affect audit/credits, not the BOM summary.
4. **Audit-time verification of the BOM.** The BOM is a report, not a gate.
   audit does not check "was the BOM generated?" — same philosophy as
   CREDITS/NOTICES (surface + comply, don't verify implementation).
5. **JSON/machine-readable output.** Markdown only for now. Can add `--format
   json` later if CI tooling needs it.

## Edge Cases & Gotchas

1. **`BOM.md` must be in `DEFAULT_EXCLUDES`.** Without this, the generated
   BOM is treated as an unlicensed asset on the next `audit` run. `CREDITS.md`
   is already excluded; `BOM.md` must be added alongside it.

2. **Use base license terms for the summary, not effective (merged) terms.**
   Per-asset overrides change what a specific asset must do, but the BOM
   describes the license itself. If an asset overrides
   `allows_commercial_use = false`, that's an asset-level fact (shown in
   credits/audit), not a license-level fact (shown in BOM).

3. **Uncovered/unknown-license assets are skipped silently.** The BOM is not
   an audit — it reports on licensed assets. If an asset has no sidecar or an
   unknown license, `audit` handles it; the BOM just omits it.

4. **Empty project.** `collect_bom` returns an empty `Vec`. `render_bom`
   renders `_No licensed assets found._`. The file is still written. `run()`
   still prints `bom: wrote {path}` and returns `Success`.

5. **Sorting.** Use `BTreeMap<String, LicenseSummary>` keyed by license id
   for stable, deterministic output (same as `credits` uses `BTreeMap` keyed
   by author). Asset lists within a summary can be sorted by path for
   determinism.

6. **Multiple distinct share-alike licenses is the only compatibility
   check.** Do not try to detect permissive-vs-copyleft conflicts or other
   license-combination analysis — that's a legal judgment, not a mechanical
   check. The one mechanical check (multiple distinct SA ids → each demands
   its own terms on derivatives → potential deadlock) is sound.

7. **`NOTICES.md` reference.** The BOM's license-notice action item says "see
   NOTICES.md". NOTICES.md doesn't exist yet (separate task). This is fine —
   the BOM points at the intended location; when NOTICES generation lands, it
   will write there. The reference is a forward pointer, not a broken link in
   the BOM's logic.

## Navigation Anchors

- `src/cli/credits_cmd.rs::run` — primary template for `bom_cmd.rs::run`.
- `src/credits.rs::collect_credits` — primary template for `bom.rs::collect_bom`.
- `src/credits.rs::generate_credits` — primary template for `bom.rs::generate_bom`.
- `src/credits.rs::build_excludes` — defense-in-depth pattern to copy.
- `src/credits.rs::render_credits` — Markdown rendering pattern to adapt.
- `src/main.rs::dispatch` — where to add the `Command::Bom` arm.
- `src/discovery.rs::DEFAULT_EXCLUDES` — where to add `"BOM.md"`.

## Dependency Mappings

No new external crates. The BOM uses the same stack as `credits`:
- `error_stack` (`Report`, `ResultExt`, `change_context`)
- `wherror` (`Error`, `#[error(debug)]`)
- `clap` (`Args`)
- internal: `crate::config::Config`, `crate::services::Services`,
  `crate::discovery::{enumerator, resolver}`,
  `crate::model::{terms, attribution, license}`,
  `crate::registry::LicenseRegistry`.

## Test Strategies

### Phase 1 (CLI wiring)

- Confirm `auditah bom --help` shows the command.
- Confirm `BOM.md` appears in `DEFAULT_EXCLUDES` (grep the const).

### Phase 2 (Collection)

- Unit test `collect_bom` with a temp tree: 2 MIT assets + 1 CC0 → 2
  summaries, correct asset counts.
- Confirm uncovered assets are skipped (no summary for them).
- Confirm unknown-license assets are skipped.

### Phase 3 (Rendering)

- Unit test `render_bom` with a synthetic `Vec<LicenseSummary>`: verify the
  header, per-license section, and action-items section structure.
- Unit test `derive_action_items` directly:
  - All-permissive → empty vec.
  - One source-disclosure license → one action item with paths.
  - One license-notice license → one action item referencing NOTICES.md.
  - One share-alike license → one action item, no conflict warning.
  - Two share-alike licenses → conflict warning + two SA items.

### Phase 4 (Integration)

- `tests/bom_pipeline.rs` with BDD tests from the test table. Use
  `temptree!` + `services_with([LicenseSpec...])` + `BomCtx` + `generate_bom`
  patterns. Assert on the rendered markdown string content (substring checks
  for section headers, license ids, action-item text).
