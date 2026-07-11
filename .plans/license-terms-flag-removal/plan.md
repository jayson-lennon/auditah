# License terms: remove FLAG severity + fix grid `manual_review`

## Problem

auditah emits a third severity, **FLAG** (non-blocking warning), for three obligations it cannot definitively resolve from repo state: license-notice shipping (`LicenseNoticeReview`), source-code offering (`SourceDisclosureReview`), and share-alike compliance (`ShareAlikeReview`). This trains users to ignore output — the "bogus license passes" failure mode the project exists to prevent — and violates the core principle that audit must **pass or fail, never silently pass**.

Separately, five shipped grids (GPL-3.0-only, LGPL-3.0-only, MPL-2.0, CC-BY-SA-4.0, OFL-1.1) carry obligations auditah cannot auto-satisfy (source disclosure, share-alike, OFL's reserved-font-name/rename restriction), yet ship `manual_review = false`. They therefore pass audit silently with no forcing function — the exact defect the manual_review mechanism exists to close.

## Solution

1. **Remove the FLAG severity entirely.** Audit output contains only FAIL findings; pass = zero findings. A previously-FLAG-only audit now reports `OK` / exit 0.
2. **Keep the terms fields** (`requires_source_disclosure`, `requires_license_notice`, `derivatives = "share-alike"`) — they document obligations, drive `credits`/`NOTICES` generation, and feed the upcoming `bom` command. They just stop producing audit findings.
3. **Fix the 5 grids** with uncheckable obligations to `manual_review = true`. These now FAIL `ManualReviewRequired` until the license id is in `manual_review_acknowledged` — the uniform forcing-function for licenses that genuinely require human engagement.
4. **No new config surface.** The existing `manual_review_acknowledged` list handles all forcing-function cases. Per-license ack is correct because the obligation is a property of the license, not the asset: acking CC-BY-SA-4.0 = "we understand its modified-material-SA obligation" and correctly auto-applies to future assets under the same license. The visibility backstop for the sticky/silent-ack property is the upcoming `bom` command (lists every distinct license in use).

## Phases

### Phase 1 — Remove FLAG severity (`src/audit/report.rs`)

Delete `Severity::Flag`, `Finding::flag`, `flag_count`, and the three review `FindingCode` variants (`ShareAlikeReview`, `SourceDisclosureReview`, `LicenseNoticeReview`). Update the `flag_only_does_not_count_as_failure` unit test (the `Severity::Flag` variant it constructs no longer exists; delete the test). Drop the `flag_count` method and its test. `has_failures()` / `fail_count()` remain the sole exit gate.

### Phase 2 — Remove FLAG producers (`src/audit.rs`)

Delete `check_manual_review_flags` (the function) and its call site at `audit.rs:155`. In the derivatives `match` (`audit.rs:241`), delete the `Derivatives::ShareAlike => report.push(Finding::flag(ShareAlikeReview, ...))` arm — the `ShareAlike` enum variant stays on `Derivatives` (it documents the obligation + drives `credits`/`bom`), it simply produces no finding. The `match` must remain exhaustive. Update doc comments that reference the FLAG path.

### Phase 3 — Fix template comment (`src/add_license.rs`)

The `requires_source_disclosure` template comment currently reads `# You MUST offer corresponding source code on distribution. Auto-unverifiable (FLAG).` Drop the `Auto-unverifiable (FLAG)` clause (FLAG no longer exists). The comment should describe the obligation, not a removed severity.

### Phase 4 — Fix the 5 grids (`well_known_licenses/*.toml`)

Set `manual_review = true` on exactly these grids (and only these):
- `GPL-3.0-only.toml` (source disclosure + share-alike)
- `LGPL-3.0-only.toml` (source disclosure + share-alike)
- `MPL-2.0.toml` (source disclosure + share-alike)
- `CC-BY-SA-4.0.toml` (share-alike on modified material)
- `OFL-1.1.toml` (reserved font name / rename restriction; share-alike)

Leave all other grids `manual_review = false`. The permissive grids (CC0-1.0, MIT, ISC, BSD-2-Clause, BSD-3-Clause, 0BSD, Apache-2.0, CC-BY-4.0, CC-BY-ND-4.0) have only auto-satisfiable or directly-checkable obligations.

### Phase 5 — Update tests

- `tests/audit_pipeline.rs::share_alike_is_flag_not_fail` — delete or rewrite. With `manual_review = true` on CC-BY-SA, this now FAILs `ManualReviewRequired`. The "share-alike alone is clean" property is now tested via an acked CC-BY-SA asset (see below).
- `tests/audit_pipeline.rs::embedded_ofl_audits_as_share_alike_flag_not_fail` — delete or rewrite. OFL now FAILs `ManualReviewRequired` unacked.
- Any test asserting `SourceDisclosureReview` or `LicenseNoticeReview` FLAGs — delete/rewrite to assert clean pass.
- Add: `cc_by_sa_acked_passes_cleanly` — CC-BY-SA-4.0 asset with id in `manual_review_acknowledged` → 0 findings (confirms share-alike alone produces no finding).
- Add: `gpl_acked_passes_cleanly` — GPL-3.0-only acked → 0 findings (confirms source-disclosure alone produces no finding).
- Update `flag_count` assertions in `src/audit/report.rs` unit tests (remove the method; those tests no longer compile).
- Confirm all FAIL-path tests (`ModifiedUnderNoDerivatives`, `NotCommerciallyLicensed`, `ManualReviewRequired` unacked, etc.) are unchanged.

### Phase 6 — Verification

Build/clippy/fmt/test clean. Re-audit `/mnt/zed/work/gamedev/assets`: CC0 packs pass, CC-BY-3.0 Gunny Sack passes (no FLAG, no FAIL). Confirm `grep` for removed symbols is empty.

## Acceptance Criteria

1. `Severity::Flag`, `Finding::flag`, `flag_count`, and `FindingCode::{ShareAlikeReview, SourceDisclosureReview, LicenseNoticeReview}` no longer exist in source (`grep` clean across `src/`).
2. `check_manual_review_flags` is removed; `derivatives = "share-alike"` alone produces no finding.
3. Audit report contains only FAIL findings; exit code is non-zero iff `fail_count() > 0`. A previously-FLAG-only audit now reports `OK` / exit 0.
4. GPL-3.0-only, LGPL-3.0-only, MPL-2.0, CC-BY-SA-4.0, OFL-1.1 ship `manual_review = true` → FAIL `ManualReviewRequired` until their id is in `manual_review_acknowledged`.
5. Permissive grids (CC0/MIT/CC-BY/BSD/ISC/Apache/0BSD/CC-BY-ND) remain `manual_review = false` → pass when attribution + text present.
6. `requires_source_disclosure`, `requires_license_notice`, and `derivatives = "share-alike"` remain on `LicenseTerms` (for `credits`/`NOTICES`/`bom`).
7. `cargo build --tests`, `cargo clippy --tests`, `cargo fmt --check`, full suite all clean.
8. Game library (`/mnt/zed/work/gamedev/assets`) re-audits with zero FLAGs; CC0 packs pass, CC-BY-3.0 Gunny Sack passes (notice obligation no longer FLAGs).

---

## Dialectical Outcomes (Why)

### The FLAG severity was the core defect

The entire terms-redesign session established typed license fields so audit could make **automated, definitive decisions** — pass or fail, no hand-waving. A FLAG ("I know there's an obligation but I'll let it pass") is precisely the "bogus license passes" failure mode the user has fought across the whole project. Decisions:

- **No third state.** A compliance tool that emits warnings trains users to ignore output. Pass or fail. `Severity::Flag` is removed entirely; `Finding::flag` is deleted.
- **Terms fields stay, audit findings go.** `requires_source_disclosure`, `requires_license_notice`, `derivatives = "share-alike"` document obligations and drive automation (`credits`/`NOTICES`) + future tooling (`bom`). They simply stop being pseudo-consumed by audit. This keeps the terms model complete for the BOM work.

### The user corrected a code-license misreading

The planner proposed `project_license` config + a "multiple distinct share-alike ids → FAIL" cross-asset check, reasoning from GPL viral-linking contagion. The user corrected both:

- **CC-BY-SA binds the *modified material*, not the project.** Modifying a CC-BY-SA mesh requires *that mesh* to ship CC-BY-SA; the game's own license is unaffected. `project_license` answers a question the obligation does not ask. Rejected.
- **GPL source disclosure is linking-dependent and undefined for assets** ("source" for a compiled `.glb` is genuinely ambiguous). Not cleanly gateable. Rejected as an audit check.

### Which obligations can audit *definitively* decide?

| Obligation | Repo-state checkable? | Verdict |
|---|---|---|
| Asset covered by sidecar/manifest | ✅ binary | FAIL check (keep) |
| License resolves in registry | ✅ binary | FAIL check (keep) |
| License text present on disk | ✅ binary | FAIL check (keep) |
| Attribution fields set when required | ✅ binary | FAIL check (keep) |
| Commercial use (config × license) | ✅ binary | FAIL check (keep) |
| Redistribution (config × license) | ✅ binary | FAIL check (keep) |
| Modified under no-derivatives | ✅ binary (contradiction) | FAIL check (keep) |
| Manual review acked | ✅ binary | FAIL check (keep) |
| **License notice shipped** | ❌ out-of-band | `credits`/`NOTICES` complies → not audit's job |
| **Source offered** | ❌ undefined for assets; linking-dependent | not gateable |
| **Modified SA material released under SA** | ❌ external licensing decision | not checkable |

The last three cannot be resolved from repo state. They become either auto-complied (notice via NOTICES) or documented (source/SA in the grid) with an opt-in gate (`manual_review`).

### `manual_review` semantics confirmed

The user confirmed: `manual_review = true` is set on grids where the licensing terms demand manual intervention (auditah cannot automatically satisfy the requirements). Per-license ack is correct because **the obligation is a property of the license, not the asset.** Acking CC-BY-SA-4.0 = "we understand its modified-material-SA obligation" and correctly auto-applies to future assets under the same license. Ack state is sticky/silent by design; the `bom` command is the visibility backstop.

### Alternatives rejected

- **`project_license` config field + cross-asset SA-incompatibility check.** Rejected: misreads CC-BY-SA scope (modified material, not project) and GPL linking.
- **`offers_source_on_distribution` / `share_alike_acknowledged` project booleans.** Rejected: fake-precise declarations for legally-murky obligations; adds config surface for things auditah can't adjudicate.
- **Derive `manual_review` from `requires_source_disclosure || derivatives == share-alike`.** Rejected: assumes auditah always knows best; license nuance (CC-BY-SA's modified-material-only scope) means the human should author the flag explicitly.
- **Keep FLAGs, reword to point at `credits`.** Rejected: still trains users to ignore non-blocking output.

---

## Relevant Files (Where)

| File | Change |
|---|---|
| `src/audit/report.rs` | Delete `Severity::Flag`, `Finding::flag`, `flag_count`, 3 review `FindingCode` variants; update unit tests |
| `src/audit.rs` | Delete `check_manual_review_flags` + call site; drop `ShareAlike => flag` arm in derivatives `match`; update doc comments |
| `src/add_license.rs` | Drop "Auto-unverifiable (FLAG)" from `requires_source_disclosure` template comment |
| `well_known_licenses/GPL-3.0-only.toml` | `manual_review = true` |
| `well_known_licenses/LGPL-3.0-only.toml` | `manual_review = true` |
| `well_known_licenses/MPL-2.0.toml` | `manual_review = true` |
| `well_known_licenses/CC-BY-SA-4.0.toml` | `manual_review = true` |
| `well_known_licenses/OFL-1.1.toml` | `manual_review = true` |
| `tests/audit_pipeline.rs` | Rewrite/delete 2 FLAG-asserting tests; add acked-clean-pass tests |
| `src/audit/report.rs` (tests) | Remove `flag_count`/`Severity::Flag` assertions |

---

## Key Code Context (What)

### `src/audit/report.rs` — the severity + ctor to delete

```rust
pub enum Severity {
    /// Blocks compliance (non-zero exit).
    Fail,
    /// Surfaces a condition that does not block.
    Flag,          // ← DELETE this variant
}

pub enum FindingCode {
    // ... checkable FAIL codes ...
    /// `derivatives = "share-alike"` — human must confirm distribution license compatibility.
    ShareAlikeReview,           // ← DELETE
    /// `requires_source_disclosure` — human must confirm source offering.
    SourceDisclosureReview,     // ← DELETE
    /// `requires_license_notice` — human must confirm license text shipped.
    LicenseNoticeReview,        // ← DELETE
    // ...
}

// Convenience constructors on Finding:
pub fn fail(code: FindingCode, asset: PathBuf, detail: impl Into<String>) -> Self { ... }   // keep
pub fn flag(code: FindingCode, asset: PathBuf, detail: impl Into<String>) -> Self { ... }    // ← DELETE

// Report methods:
pub fn has_failures(&self) -> bool { ... }     // keep (exit gate)
pub fn fail_count(&self) -> usize { ... }      // keep
pub fn flag_count(&self) -> usize { ... }      // ← DELETE
```

### `src/audit.rs` — the producer to delete

```rust
// In check_obligations / derivatives dispatch (audit.rs:155):
check_manual_review_flags(asset, terms, report);   // ← DELETE this call

// In the derivatives match (audit.rs:230):
match terms.derivatives {
    Derivatives::Disallowed => { /* FAIL if modified — KEEP */ }
    Derivatives::Allowed => {}
    Derivatives::ShareAlike => {
        report.push(Finding::flag(                // ← DELETE this whole arm
            FindingCode::ShareAlikeReview,
            asset.to_path_buf(),
            "license requires share-alike; confirm distribution license compatibility",
        ));
    }
}
// (The ShareAlike variant stays on the Derivatives enum — just produces no finding.)

// This entire function (audit.rs:275) is deleted:
fn check_manual_review_flags(
    asset: &Path,
    terms: &crate::model::terms::LicenseTerms,
    report: &mut AuditReport,
) {
    if terms.requires_source_disclosure {
        report.push(Finding::flag(FindingCode::SourceDisclosureReview, ...));
    }
    if terms.requires_license_notice {
        report.push(Finding::flag(FindingCode::LicenseNoticeReview, ...));
    }
}
```

### `src/audit.rs` — `check_manual_review` (the FAIL gate, UNCHANGED)

```rust
fn check_manual_review(
    asset: &Path,
    license_id: &str,
    terms: &crate::model::terms::LicenseTerms,
    config: &Config,
    report: &mut AuditReport,
) {
    let acknowledged = config.manual_review_acknowledged.iter().any(|id| id == license_id);
    if terms.manual_review && !acknowledged {
        report.push(Finding::fail(
            FindingCode::ManualReviewRequired,
            asset.to_path_buf(),
            format!("license {license_id:?} requires manual review; add it to ..."),
        ));
    }
}
```

### Grid file shape (`well_known_licenses/*.toml`) — the one-line change

```toml
# Before (GPL-3.0-only.toml, LGPL-3.0-only.toml, MPL-2.0.toml, CC-BY-SA-4.0.toml, OFL-1.1.toml)
manual_review = false

# After
manual_review = true
```

---

## Implementation Algorithm (How)

1. **`src/audit/report.rs`** — remove `Severity::Flag` variant. Delete `Finding::flag` ctor. Delete `flag_count`. Delete `FindingCode::{ShareAlikeReview, SourceDisclosureReview, LicenseNoticeReview}`. The `Finding` struct's `severity` field is now implicitly `Fail` (or collapse the field). Re-run unit tests in this module: delete `flag_only_does_not_count_as_failure` and any `flag_count` assertion. `has_failures()`/`fail_count()` remain the exit gate.

2. **`src/audit.rs`** — delete the `check_manual_review_flags` function entirely. Remove its call at `audit.rs:155`. In the derivatives `match`, delete the `Derivatives::ShareAlike` arm's `push(flag)` body; the arm becomes `Derivatives::ShareAlike => {}` (variant retained, no finding). Confirm the match stays exhaustive. Update module/function doc comments that mention the FLAG path or "surface for human action."

3. **`src/add_license.rs`** — find the `requires_source_disclosure` template comment (~line 133) and drop the `Auto-unverifiable (FLAG)` clause.

4. **Grids** — for each of the 5 files in `well_known_licenses/`, flip `manual_review = false` → `manual_review = true`. Touch nothing else.

5. **Tests** — in `tests/audit_pipeline.rs`:
   - Delete or rewrite `share_alike_is_flag_not_fail` and `embedded_ofl_audits_as_share_alike_flag_not_fail` (the behavior they assert no longer exists; CC-BY-SA/OFL now FAIL `ManualReviewRequired` unacked).
   - Add `cc_by_sa_acked_passes_cleanly`: build a CC-BY-SA-4.0 asset, add `"CC-BY-SA-4.0"` to `manual_review_acknowledged`, assert 0 findings (confirms share-alike alone produces nothing post-removal).
   - Add `gpl_acked_passes_cleanly`: same for GPL-3.0-only, asserting `requires_source_disclosure` produces nothing.
   - In `src/audit/report.rs` tests, remove `flag_count` usage and the `flag_only_does_not_count_as_failure` test.

6. **Verify** — `cargo build --tests && cargo clippy --tests -- -D warnings && cargo fmt --check && cargo nextest run`. Then re-audit the game library: `auditah audit --root /mnt/zed/work/gamedev/assets` → expect CC0 packs pass, CC-BY-3.0 Gunny Sack passes (no FLAG). Then `grep -rn 'Severity::Flag\|Finding::flag\|flag_count\|ShareAlikeReview\|SourceDisclosureReview\|LicenseNoticeReview' src/` → expect empty.

---

## Anti-Goals (Out of Scope)

- **No `project_license` config field.** Rejected: misreads CC-BY-SA scope.
- **No `offers_source_on_distribution` / `share_alike_acknowledged` project booleans.** Rejected: fake-precise for legally-murky obligations.
- **No deriving `manual_review` from other fields.** The flag is authored explicitly per grid.
- **No review-date tracking / auto-re-surface for acks.** The sticky/silent-ack property is accepted; the `bom` command is the visibility backstop.
- **No changes to `check_obligations` FAIL paths** (attribution, commercial, redistribution, no-derivatives, manual-review-unacked). Those are the definitive checks and stay exactly as-is.
- **No BOM command.** Mentioned as the next task; not built here. The terms fields are retained *for* it.

---

## Edge Cases & Gotchas

- **The `Derivatives::ShareAlike` enum variant must stay on the enum**, even though it no longer produces an audit finding. It documents the obligation, distinguishes the grid for authors, and drives `credits`/`NOTICES`/`bom`. Deleting it would break the terms model and the "unrepresentable states" guarantee (it's the third arm that prevents ND+SA contradictions). The audit `match` arm becomes empty-bodied, not removed.
- **The 5 grid fixes change observable registry behavior.** Any test that audits GPL/OFL/CC-BY-SA/MPL/LGPL *without* acking them will newly FAIL `ManualReviewRequired`. The two audit-pipeline FLAG tests hit this directly. Audit any other test that references these 5 ids.
- **`#[serde(deny_unknown_fields)]`** on `LicenseTerms` means the grid edits are pure value flips — no structural risk. But the `manual_review` field stays (it's a real field, not deleted); only its value changes on 5 grids.
- **`Severity` may collapse to a single-variant enum or be removed.** If `Finding` still carries a `severity: Severity` field and `Severity` has only `Fail` left, consider whether to keep the enum (for forward-compat) or inline. Recommend keeping the enum with one variant for minimal churn; do not over-refactor in this task.
- **CC-BY-ND-4.0 stays `manual_review = false`.** Its only obligation (`derivatives = "disallowed"`) is directly checkable via `ModifiedUnderNoDerivatives`. Do not set it `true`.
- **Game-library re-audit expectation:** the CC-BY-3.0 Gunny Sack asset previously FLAGged `LicenseNoticeReview`. Post-change it passes cleanly (0 findings) because the CC-BY-3.0 grid has `manual_review = false` and the notice obligation no longer produces a finding. This is the intended UX win.

---

## Navigation Anchors

- **`src/audit/report.rs`** — `Severity`, `FindingCode`, `Finding`, `AuditReport`. Primary edit site for the severity removal.
- **`src/audit.rs::check_obligations`** — the per-asset check orchestrator; calls `check_manual_review_flags` (to delete) and contains the derivatives `match`.
- **`src/audit.rs::check_manual_review`** — the FAIL gate (unchanged); the model for how forcing functions work.
- **`src/model/terms.rs::Derivatives`** — the enum whose `ShareAlike` variant stays but goes finding-less.
- **`well_known_licenses/`** — the 5 grid files to flip.
- **`tests/audit_pipeline.rs`** — integration tests for audit behavior; the 2 FLAG tests to rewrite + 2 acked-clean tests to add.

---

## Dependency Mappings

- **No new external dependencies.** Pure subtraction + value flips + test updates.
- **Internal dependencies unchanged.** `LicenseTerms`, `Derivatives`, `Config`, `Finding`, `AuditReport` all retain their shapes (minus the FLAG members).
- **Forward dependency: `bom` command (next task).** It will consume `requires_source_disclosure`, `requires_license_notice`, and `derivatives = "share-alike"` to produce the bill of materials. Retaining these fields is the handoff.

---

## Test Strategies

- **Unit (`src/audit/report.rs`):** after removing `Severity::Flag`/`Finding::flag`/`flag_count`, delete `flag_only_does_not_count_as_failure` and any `flag_count` assertion. Confirm `has_failures()`/`fail_count()` tests still pass. Confirm the module compiles (the enum variant removal will surface any stray `Severity::Flag` references as compile errors — fix them).
- **Integration (`tests/audit_pipeline.rs`):** the 2 FLAG tests are the critical edits. Convert them to the new model: unacked CC-BY-SA/OFL → FAIL `ManualReviewRequired`; acked → 0 findings. Add explicit acked-clean tests for CC-BY-SA-4.0 and GPL-3.0-only to lock in "share-alike/source-disclosure alone produce no finding."
- **Grid correctness:** add/confirm a test that the 5 fixed grids parse with `manual_review = true` and the 9 permissive grids parse with `manual_review = false` (rstest over `well_known_licenses/*.toml` if not already present).
- **End-to-end (game library):** `auditah audit --root /mnt/zed/work/gamedev/assets` → CC0 packs pass, CC-BY-3.0 Gunny Sack passes (0 findings). No FLAG in output.
- **Grep gate:** `grep -rn 'Severity::Flag\|Finding::flag\|flag_count\|ShareAlikeReview\|SourceDisclosureReview\|LicenseNoticeReview' src/` returns empty.
