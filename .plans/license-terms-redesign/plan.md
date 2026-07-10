# License Terms Model Redesign — Specification

## Problem

The `LicenseTerms` struct (`src/model/terms.rs`) models derivatives as two
independent bools — `allows_modifications` and `requires_share_alike`. This
permits two contradictory states by construction:

- `allows_modifications = false` + `requires_modification_notice = true`
  (a dead obligation: announce a modification you're forbidden to make)
- `allows_modifications = false` + `requires_share_alike = true`
  (a vacuous clause: share-alike fires on derivatives, which are banned)

The struct comment at `src/model/terms.rs:14` claims
"Term flags are independent by design" — this premise is false; the two
contradictions share one root cause: share-alike and modification-notice are
not peers of `allows_modifications`, they are *attributes of the permitted
state*.

The model also cannot express two things critical to asset-license
compliance:

- **Redistribution restrictions** — the defining clause of asset-store EULAs
  ("use in your shipped product, don't re-host the raw asset").
- **Unverifiable bespoke clauses** — seat limits, territory, field-of-use,
  reserved font names — clauses the boolean grid cannot express, with no
  signal to surface them.

## Solution

1. **`Derivatives` string enum** (`disallowed | allowed | share-alike`)
   replaces `allows_modifications` + `requires_share_alike`. Illegal states
   become literally unconstructable: you write one variant, not two
   coordinated bools.
2. **`allows_redistribution: bool`** added to `LicenseTerms` (overridable per
   asset via `Overrides`), gated by a new **`redistributes_assets: bool`**
   project-config flag — mirroring the existing
   `commercial_project` ↔ `allows_commercial_use` pattern.
3. **`manual_review: bool`** escape hatch on `LicenseTerms` (NOT overridable —
   license-only property). It **fails CI until** the license id appears in a
   new **`manual_review_acknowledged: Vec<String>`** config field (fail-closed
   with a human resolution path). The acknowledgment list replaces the
   separate "escalate to FAIL" knob — it *is* the strictness mechanism.
4. TOML stays flat. Old keys (`allows_modifications`, `requires_share_alike`)
   are **rejected** via `#[serde(deny_unknown_fields)]` — clean break, no
   consumers yet.

## Acceptance Criteria

- `cargo build --tests` clean; `cargo clippy --tests` clean; full suite green.
- No code path can construct or write a TOML expressing
  "no-derivatives + share-alike."
- OFL/CC-BY/CC0/MIT round-trip and audit identically to today.
- A `redistributes_assets = true` project FAILs an asset with
  `allows_redistribution = false`.
- A `manual_review = true` license FAILs until its id appears in
  `manual_review_acknowledged`, then is clean.
- A stale TOML carrying `allows_modifications` or `requires_share_alike` is a
  parse error, not silently ignored.
- `manual_review` is not settable in `Overrides` (license-only property).
- `effective_terms` remains infallible (returns `LicenseTerms`, not `Result`).

## Test Cases

| # | Case | Expected |
|---|---|---|
| 1 | OFL (share-alike) audits, emits `ShareAlikeReview` FLAG | FLAG, non-blocking |
| 2 | `derivatives = "disallowed"` + `modified = true` | FAIL `ModifiedUnderNoDerivatives` |
| 3 | `derivatives = "allowed"` + `modified = true` | clean (no derivatives finding) |
| 4 | Stale TOML with `allows_modifications` key | parse error at registry load |
| 5 | `redistributes_assets = true` + asset `allows_redistribution = false` | FAIL `RedistributionViolation` |
| 6 | `redistributes_assets = false` + same asset | clean (gate inactive) |
| 7 | `manual_review = true`, not acknowledged | FAIL `ManualReviewRequired` |
| 8 | `manual_review = true`, id in `manual_review_acknowledged` | clean |
| 9 | `manual_review = false` | clean regardless of ack list |
| 10 | `effective_terms` with `derivatives: Some(Disallowed)` override | merges cleanly, no validation needed |
| 11 | Override writer (`add`) emits `derivatives = "share-alike"` as string, round-trips | round-trip equality |
| 12 | All 4 embedded licenses load + audit unchanged from today | regression parity |

---

## Phases

### Phase 1 — Model (`src/model/terms.rs`)

Add the `Derivatives` enum with `#[serde(rename_all = "kebab-case")]`. Rewrite
`LicenseTerms`: drop `allows_modifications` + `requires_share_alike`; add
`derivatives: Derivatives`, `allows_redistribution: bool`, `manual_review: bool`.
Apply `#[serde(deny_unknown_fields)]` to `LicenseTerms` AND `Overrides`. In
`Overrides`: add `derivatives: Option<Derivatives>` and
`allows_redistribution: Option<bool>`; **do NOT add `manual_review`** (it is a
license-only property and must not be overridable). Keep
`effective_terms` infallible (`unwrap_or` per field). Rewrite the module's
unit tests (`cc_by_terms()` helper + the three override tests).

### Phase 2 — Config (`src/config.rs`)

Add two fields to `Config`, both `#[serde(default)]`:
`redistributes_assets: bool` and `manual_review_acknowledged: Vec<String>`.
Update `Default` derive (implicit) and the config unit tests.

### Phase 3 — Audit (`src/audit.rs` + `src/audit/report.rs`)

In `check_obligations`: replace the two derivatives bool-checks with an
exhaustive `match terms.derivatives`; add the redistribution gate; add the
manual-review fail-closed gate (requires `license_id` passed into the function).
Add `FindingCode` variants `RedistributionViolation` and `ManualReviewRequired`.

### Phase 4 — Writers (`src/add.rs`)

In `has_any_override` and `override_table`: add `derivatives` and
`allows_redistribution` branches (both Option). `derivatives` serializes as a
string. Do NOT add `manual_review`. Update the module's override round-trip
tests.

### Phase 5 — Embedded licenses (`src/embedded_licenses/*.toml`)

Rewrite all 4 `[terms]` blocks to the new shape. Map: OFL →
`derivatives = "share-alike"`; CC0/CC-BY/MIT → `derivatives = "allowed"`.
All four get `allows_redistribution = true` and `manual_review = false`. The
OFL reserved-font-name comment moves to the registry entry's `notes` (already
supported, no model change).

### Phase 6 — Tests + docs

Update `tests/common/mod.rs` config helpers (`non_commercial_config`,
`commercial_config` gain the new fields). Grep the test tree for the removed
field names (`allows_modifications`, `requires_share_alike`) and migrate every
inline TOML blob: `audit_pipeline.rs`, `obligation_rstest.rs`,
`error_scenarios.rs`, `licenses_dir_pipeline.rs`, `add_pipeline.rs`. Update the
`README.md` terms table. Add new-case tests per the table above.

---

## Dialectical Outcomes (Why)

### Derivatives as an enum, not validated bools
The alternative (keep 7 bools, add a `validate()`) was rejected because it only
*rejects* contradictions at runtime — it does not make them unrepresentable.
The user's explicit goal: "every construction of TOML configuration is
completely valid, as long as there are no typos." An enum satisfies this by
construction; a validator does not.

### Flat TOML with a string value, not a nested table
`derivatives = "share-alike"` deserializes directly into the typed enum via
serde with no validation layer. The earlier two-type
(`RawLicenseTerms` → `TryFrom`) scaffolding was for the rejected keep-bools
option — the enum kills the need for it entirely. `effective_terms` stays pure
`unwrap_or`, never returns `Result`.

### Share-alike is a derivative variant, not a separate flag
Share-alike only has meaning when modification is permitted — it is an
attribute of the permitted state. Lifting it into the enum dissolves the
override-merge contradiction problem entirely: an override changes the
*variant*, always producing another *valid* variant. No post-merge validation
is ever needed.

### `requires_modification_notice` stays a standalone bool
Tracing showed `audit.rs` never reads it; `credits.rs` reads it *only* to
render the cosmetic `(modified from original)` string. It is independent,
non-coordinating, and inert under `derivatives = "disallowed"` (an ND asset is
never marked modified). Folding it into the enum would double the variants for
a cosmetic field.

### The four core bools stay bools
`requires_attribution`, `requires_license_notice`, `requires_source_disclosure`,
`allows_commercial_use` — a bool cannot be put into a contradictory *form*
(only one form per value). They already satisfy "valid as long as no typos."

### `manual_review` replaces the OpenSource/Proprietary classifier
The classifier was too blunt: "copyright X; use however you want" is
proprietary *by taxonomy* but fully reducible to the grid and should not be
flagged. The real axis is "does this license reduce to the grid, or carry
clauses the grid can't express?" So: every license is either expressible
(auto-audited) or self-declaredly-not (`manual_review = true`). Bespoke-
permissive EULAs that fit never set the flag.

### `allows_redistribution` is the one EULA axis worth typing
It is the defining clause of asset-store EULAs and OS licenses don't model it
(OS *mandates* redistribution). Everything else common in EULAs (seats,
territory, field-of-use, training-data) isn't reducible to a bool without a
bespoke tail — those stay behind `manual_review + notes` by design.

### Redistribution is dead weight without its config gate
`allows_redistribution` would never be read without a matching
`redistributes_assets` switch in `auditah.toml` — exactly as
`allows_commercial_use` only matters when `commercial_project = true`. The two
must ship together or neither ships.

### Manual review is fail-closed (unacked → FAIL), not a non-blocking FLAG
Earlier rejected as "every CI would always fail." Re-accepted once the
acknowledgment list exists: it *is* the human resolution path. The FAIL message
names the fix. This kills the separate `treat_manual_review_as_failure` knob —
the ack list *is* the strictness mechanism. Fail-closed is the safer default
for a compliance tool.

### Acknowledgment is permanent and silent
Once an id is listed, the "needs review" signal disappears from the report.
No review-date tracking, no auto-re-surface on license-text change. Accepted as
out of scope.

### `manual_review` is NOT in `Overrides`
"This license can't be auto-verified" is a property of *the license*, not of
an asset's use of it. Letting one asset silently turn it off defeats the
fail-closed guarantee.

### Clean break, no legacy key tolerance
The project is pre-release with no consumers. Stale keys must fail loudly
(`deny_unknown_fields`), not silently degrade.

### Scope: assets only
Patent grants/retaliation, AGPL network-use, LGPL conditional copyleft, dual
licensing, trademark reservation, field-of-use — explicitly out of scope as
typed fields.

---

## Relevant Files (Where)

**Modify:**
- `src/model/terms.rs` — `Derivatives` enum, `LicenseTerms`, `Overrides`, `effective_terms`
- `src/model/license.rs` — test helper `cc0_entry()` constructs `LicenseTerms`
- `src/registry.rs` — test inline TOML blobs + embedded-load tests
- `src/config.rs` — two new `Config` fields + tests
- `src/audit.rs` — `check_obligations` rewrite
- `src/audit/report.rs` — new `FindingCode` variants
- `src/add.rs` — `has_any_override`, `override_table` + tests
- `src/embedded_licenses/CC0-1.0.toml`
- `src/embedded_licenses/CC-BY-3.0.toml`
- `src/embedded_licenses/MIT.toml`
- `src/embedded_licenses/OFL-1.1.toml`
- `tests/common/mod.rs` — config helpers
- `tests/audit_pipeline.rs` — case 9 (allows_modifications) + case 10 (requires_share_alike)
- `tests/obligation_rstest.rs` — `modified_under_no_derivatives` case
- `tests/error_scenarios.rs` — inline license TOML
- `tests/licenses_dir_pipeline.rs` — inline license TOML
- `tests/add_pipeline.rs` — override assertions
- `README.md` — terms table

**No change:**
- `src/credits.rs` — reads only `requires_modification_notice` (unchanged bool) via `effective_terms`
- `tests/credits_pipeline.rs` — uses only `requires_modification_notice` (unchanged)

---

## Key Code Context (What)

### Current `LicenseTerms` + `Overrides` (`src/model/terms.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // <-- this premise is FALSE; remove the allow
pub struct LicenseTerms {
    pub requires_attribution: bool,
    pub requires_license_notice: bool,
    pub requires_source_disclosure: bool,
    pub requires_share_alike: bool,          // REMOVE
    pub requires_modification_notice: bool,
    pub allows_commercial_use: bool,
    pub allows_modifications: bool,          // REMOVE
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_attribution: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_license_notice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_source_disclosure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_share_alike: Option<bool>,  // REMOVE
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_modification_notice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_commercial_use: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_modifications: Option<bool>,  // REMOVE
}
```

### Current `effective_terms` — stays infallible, same `unwrap_or` pattern

```rust
#[must_use]
pub fn effective_terms(base: &LicenseTerms, overrides: &Overrides) -> LicenseTerms {
    LicenseTerms {
        requires_attribution: overrides.requires_attribution.unwrap_or(base.requires_attribution),
        // ... one unwrap_or per field ...
    }
}
```

### Current `Config` (`src/config.rs`)

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub commercial_project: bool,
    #[serde(default)]
    pub exclude: Vec<String>,
}
```

### Current `check_obligations` derivatives checks (`src/audit.rs`)

```rust
// Modifications boundary.
if record.modified && !terms.allows_modifications {
    report.push(Finding::fail(
        FindingCode::ModifiedUnderNoDerivatives,
        asset.to_path_buf(),
        "asset is modified but license disallows derivatives",
    ));
}
// Manual-review flags.
if terms.requires_share_alike {
    report.push(Finding::flag(
        FindingCode::ShareAlikeReview, asset.to_path_buf(),
        "license requires share-alike; confirm distribution license compatibility",
    ));
}
```

`check_obligations` signature is `fn check_obligations(asset, record, terms, config, report)`.
It currently does NOT receive the license id. The manual-review ack check needs
the id, so either pass `license_id: &str` in (it's available at the call site as
`record.license.as_str()` / `&entry.id`), or check `record.license`. The call
site (`run_audit`):

```rust
if let Some(entry) = ctx.services.registry.get(&record.license) {
    check_license_text(asset, &entry.id, ctx, &mut report);
    let terms = effective_terms(&entry.terms, &record.overrides);
    check_obligations(asset, record, &terms, ctx.config, &mut report);
}
```

### Current commercial-use gate (the template for the redistribution gate)

```rust
// Commercial use boundary.
if config.commercial_project && !terms.allows_commercial_use {
    report.push(Finding::fail(
        FindingCode::NotCommerciallyLicensed,
        asset.to_path_buf(),
        "project is commercial but asset is not licensed for commercial use",
    ));
}
```

### Current `FindingCode` (`src/audit/report.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingCode {
    UnlicensedAsset, OrphanSidecar, UnknownLicense, IncompleteAttribution,
    NotCommerciallyLicensed, ModifiedUnderNoDerivatives,
    ShareAlikeReview, SourceDisclosureReview, LicenseNoticeReview,
    MissingLicenseText,
}
```

### Current `override_table` + `has_any_override` (`src/add.rs`)

```rust
fn has_any_override(o: &Overrides) -> bool {
    o.requires_attribution.is_some()
        || o.requires_license_notice.is_some()
        || o.requires_source_disclosure.is_some()
        || o.requires_share_alike.is_some()
        || o.requires_modification_notice.is_some()
        || o.allows_commercial_use.is_some()
        || o.allows_modifications.is_some()
}

fn override_table(o: &Overrides) -> toml_edit::Item {
    let mut t = table();
    // ... one `if let Some(v)` branch per field ...
    t
}
```

---

## Implementation Algorithm (How)

### Derivatives enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Derivatives {
    Disallowed,
    Allowed,
    ShareAlike,
}
```

TOML representation: `disallowed` / `allowed` / `share-alike`.

### New `LicenseTerms`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseTerms {
    pub requires_attribution: bool,
    pub requires_license_notice: bool,
    pub requires_source_disclosure: bool,
    pub derivatives: Derivatives,
    pub requires_modification_notice: bool,
    pub allows_commercial_use: bool,
    pub allows_redistribution: bool,
    pub manual_review: bool,
}
```
Remove the `#[allow(clippy::struct_excessive_bools)]` — the struct now has an
enum plus bools; bool count dropped, and the allow was justified by a false
premise. (If clippy still warns on bool count, evaluate at implementation time.)

### New `Overrides` (note: no `manual_review`)

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_attribution: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_license_notice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_source_disclosure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivatives: Option<Derivatives>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_modification_notice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_commercial_use: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_redistribution: Option<bool>,
}
```

### `effective_terms` — infallible, `unwrap_or` per field

Each new field uses `unwrap_or` over the base, exactly like existing fields.
`derivatives` is `Option<Derivatives>::unwrap_or(base.derivatives)`.

### `check_obligations` rewrite

Replace the modifications-boundary + share-alike-flag block with an exhaustive
match:

```rust
match terms.derivatives {
    Derivatives::Disallowed => {
        if record.modified {
            report.push(Finding::fail(
                FindingCode::ModifiedUnderNoDerivatives, asset.to_path_buf(),
                "asset is modified but license disallows derivatives"));
        }
    }
    Derivatives::Allowed => {}
    Derivatives::ShareAlike => {
        report.push(Finding::flag(
            FindingCode::ShareAlikeReview, asset.to_path_buf(),
            "license requires share-alike; confirm distribution license compatibility"));
    }
}
```

Add the redistribution gate (mirror of commercial gate):

```rust
if config.redistributes_assets && !terms.allows_redistribution {
    report.push(Finding::fail(
        FindingCode::RedistributionViolation, asset.to_path_buf(),
        "project redistributes assets but license forbids redistribution"));
}
```

Add the manual-review fail-closed gate. `check_obligations` needs the license
id — add a `license_id: &str` parameter (pass `&entry.id` from `run_audit`):

```rust
if terms.manual_review && !config.manual_review_acknowledged.iter().any(|id| id == license_id) {
    report.push(Finding::fail(
        FindingCode::ManualReviewRequired, asset.to_path_buf(),
        format!("license {license_id:?} requires manual review; add it to \
                 `manual_review_acknowledged` in auditah.toml after review")));
}
```

Keep the existing `requires_source_disclosure` and `requires_license_notice`
FLAG blocks unchanged.

### `check_obligations` signature change

```rust
fn check_obligations(
    asset: &Path,
    record: &AttributionRecord,
    license_id: &str,          // NEW
    terms: &LicenseTerms,
    config: &Config,
    report: &mut AuditReport,
)
```
Call site passes `&entry.id`.

### Config additions

```rust
#[serde(default)]
pub redistributes_assets: bool,
#[serde(default)]
pub manual_review_acknowledged: Vec<String>,
```

### Override writer (`add.rs`)

`has_any_override` gains `|| o.derivatives.is_some() || o.allows_redistribution.is_some()`
(loses the two removed fields). `override_table` gains:

```rust
if let Some(v) = o.derivatives {
    t["derivatives"] = value(serde_plain::to_string(v).unwrap()); // or match-to-string
}
if let Some(v) = o.allows_redistribution {
    t["allows_redistribution"] = value(v);
}
```
For the string conversion without a new dependency, serialize `Derivatives` via
a small `match` to its kebab string, or use `toml::to_string` on a temp value.
Do NOT add a dependency for this.

### Embedded license mapping

| License | derivatives | attribution | notice | source | comm | mod-notice | redist | review |
|---|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| CC0-1.0 | allowed | ✗ | ✗ | ✗ | ✓ | ✗ | ✓ | ✗ |
| MIT | allowed | ✗ | ✓ | ✗ | ✓ | ✗ | ✓ | ✗ |
| CC-BY-3.0 | allowed | ✓ | ✗ | ✗ | ✓ | ✓ | ✓ | ✗ |
| OFL-1.1 | share-alike | ✗ | ✓ | ✗ | ✓ | ✗ | ✓ | ✗ |

All four: `allows_redistribution = true`, `manual_review = false`.

---

## Edge Cases & Gotchas

- **`#[serde(deny_unknown_fields)]` on `Overrides`:** This struct is deserialized from a nested `[overrides]` table inside an `AttributionRecord` sidecar. Ensure `deny_unknown_fields` is applied to `Overrides` itself (the nested table), not the parent. Verify round-trip tests in `add.rs` still parse.
- **`manual_review` ack matching is by exact string id** — `manual_review_acknowledged` contains SPDX ids like `"LicenseRef-StudioEULA"`. The check compares against `entry.id`. Case-sensitive, exact match.
- **`check_obligations` signature change** ripples to its single call site in `run_audit`. There are no other callers.
- **`requires_modification_notice` is NOT touched** — it stays a bool in both structs. `credits.rs` reads it; do not modify credits logic.
- **OFL reserved-font-name** is NOT modeled (no field). Its comment should move to the entry `notes` (a `notes: Option<String>` already exists on `LicenseRegistryEntry`). This is documentation-only; no behavior change.
- **Clippy `struct_excessive_bools`:** after removing two bools and adding one enum + two bools, the bool count is unchanged-ish. If the lint still fires, prefer NOT silencing it with a stale comment — restructure only if it fires.
- **Stale embedded TOML = compile-time panic:** `registry.rs::embedded_entries()` `panic!`s on malformed embedded TOML. After rewriting the 4 TOMLs, the binary must boot. Run `cargo test registry` immediately after Phase 5.
- **`deny_unknown_fields` + `#[serde(default)]` interaction:** `deny_unknown_fields` is compatible with `default` on present fields; it only rejects *unknown* keys. Confirm the override round-trip still works (the writer must not emit the removed keys).

---

## Navigation Anchors

- **Primary entry point:** `src/model/terms.rs` — `LicenseTerms`, `Overrides`, `Derivatives`, `effective_terms`.
- **Audit logic:** `src/audit.rs` → `check_obligations` (the obligation state machine) + `run_audit` (the call site that passes the new `license_id`).
- **Findings:** `src/audit/report.rs` → `FindingCode`.
- **TOML write path:** `src/add.rs` → `render_record` → `override_table` / `has_any_override`.
- **Config:** `src/config.rs` → `Config`.
- **Embedded data:** `src/registry.rs` → `embedded_entries()` + `src/embedded_licenses/*.toml`.

---

## Dependency Mappings

**No new external dependencies.** The `Derivatives` enum serializes via existing
`serde` (already a dependency). String conversion in `override_table` uses a
`match` (no `serde_plain`). `toml_edit` is already present for the writer.

---

## Test Strategies

- **Phase 1 (terms.rs unit tests):** update `cc_by_terms()` helper; keep the three existing override tests (they now assert `derivatives` inheritance via `Some(...)` override). Add a test: `derivatives: Some(Disallowed)` override on an `Allowed` base produces `Disallowed`. Add a deny-unknown-fields test: deserializing a TOML with `allows_modifications` returns `Err`.
- **Phase 2 (config.rs unit tests):** add a test parsing `redistributes_assets = true` and `manual_review_acknowledged = ["LicenseRef-X"]`.
- **Phase 3 (audit):** extend `tests/obligation_rstest.rs` with cases for `RedistributionViolation` and `ManualReviewRequired` (and the ack-cleared variant). These follow the existing `#[case]` parameterized pattern.
- **Phase 4 (add.rs):** update the override round-trip test to set `derivatives` and `allows_redistribution`; assert `manual_review` is absent from `Overrides` (a compile-time guarantee).
- **Phase 5 (embedded):** run `cargo test registry` immediately — `embedded_entries()` will panic at test boot if any TOML is malformed.
- **Phase 6 (integration):** migrate inline TOML in pipeline tests by replacing the two removed keys with the single `derivatives` key. Grep: `rg 'allows_modifications|requires_share_alike'` must return zero hits in `tests/` and `src/` after migration (except possibly the new deny-unknown-fields test fixture, which intentionally uses a stale key).
- **Regression:** the existing OFL share-alike FLAG case (audit_pipeline case 10) must still produce `ShareAlikeReview`; the modified-under-ND case (case 9) must still FAIL.

---

## Anti-Goals (Out of Scope)

- **No review-date / audit-trail tracking** for acknowledged licenses. Ack = permanent silent clean.
- **No auto-re-surface** when license text changes after acknowledgment.
- **No typed fields** for patent, trademark, field-of-use, territory, seat limits, AGPL network-use, LGPL conditional copyleft, or dual licensing. These stay in free-form `notes` / behind `manual_review`.
- **No attribution-placement field.** (Withdrawn during dialogue — placement is a CC recommendation, not a legal term; only matters in proprietary EULAs, where it belongs in `notes`.)
- **No `treat_manual_review_as_failure` config knob.** The acknowledgment list replaces it.
- **No legacy/compat dual parsing** of old bool keys. Clean break only.
- **No surfacing of `notes` in credits/audit output** in this work. (Orthogonal one-line follow-up; defer.)
- **No code-dependency auditing.** Assets only.
