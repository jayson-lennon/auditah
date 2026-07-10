# Lint hardening: eliminate production `unwrap`/`expect`, harden override/table-construction tests

## Problem

Two production `expect()` calls would **panic** (SIGABRT) on invalid user input, bypassing the clean exit-2 error path:

- `src/audit.rs:80` — `ExcludeMatcher::new(&patterns).expect("…validated at config load")`
- `src/credits.rs:82` — `ExcludeMatcher::new(&patterns).expect("…")`

The audit.rs comment *claims* the globs are "validated at config load," but `Config::load` (`src/config.rs:47-60`) only runs `toml::from_str` — it never compiles the globs. So a user who writes `exclude = ["**/[invalid"]` in `auditah.toml` sails through config load, reaches `build_excludes`, and **panics**. The existing test `tests/error_scenarios.rs:85` (`exclude_matcher_rejects_invalid_glob_pattern`) proves an invalid glob is constructible, yet nothing rejects it before the `expect`.

Separately, the override/table-construction code (`src/add.rs`: `render_record`, `override_table`, `has_any_override`, `derivatives_to_kebab`) and the resolver parse path (`src/discovery/resolver.rs::read_attribution`) are under-tested for *value/enum and unknown-field* failure modes — happy paths and syntax/missing-field errors are covered, but several branches are not exercised.

## Solution

Two layers of defense against the panic, plus restriction lints to prevent regression, plus targeted tests:

1. **Eager validation in `Config::load`** (fail-fast UX): compile the exclude globs immediately after `toml::from_str`; surface invalid patterns as `ConfigError` at the right locus (a bad `auditah.toml` is a config error) before any audit/credits work begins.
2. **Make `build_excludes` fallible** (panic elimination + defense in depth): both `build_excludes` functions return `Result<ExcludeMatcher, Report<_>>` and propagate via `?`. This eliminates the panic *and* stays safe even when `Config` is constructed directly (tests do this), bypassing `load`.
3. **Enable `clippy::unwrap_used` / `clippy::expect_used` as `warn`** in `[lints.clippy]`, scoped out of test code via `#[allow(...)]` per test module / `#![allow(...)]` per integration-test file. Project treats warnings as failures (zero-warning CI), so this enforces "no production panics."
4. **Add four override/table-construction tests** closing the value/enum and unknown-field coverage gaps through the real writer + resolver paths.

## Acceptance Criteria

- `cargo clippy --tests` is clean with `clippy::unwrap_used` and `clippy::expect_used` enabled — zero warnings.
- No `unwrap`/`expect` remains in production code paths (everything outside `#[cfg(test)] mod tests` and the `test-helper`-gated `test_support.rs`, and outside `tests/`).
- `Config::load` returns `Err` for an `auditah.toml` with an invalid `exclude` glob (fail-fast).
- `run_audit` and `collect_credits` propagate a `build_excludes` failure cleanly as `Err` (exit 2), never panic.
- `derivatives_to_kebab` round-trips all three `Derivatives` variants.
- The full override table (all 7 fields set) round-trips through `render_record`.
- The resolver errors on a sidecar with an **invalid `derivatives` value**.
- The resolver errors on a sidecar with an **unknown `[overrides]` field** (proves `deny_unknown_fields` holds through the real parse path).
- Full test suite green.

## Test Cases

| # | Case | Expected |
|---|---|---|
| 1 | `auditah.toml` with `exclude = ["**/[invalid"]` → `Config::load` | `Err(ConfigError)` (fail-fast) |
| 2 | `Config` constructed directly with a bad exclude + `run_audit` | `Err` (no panic) |
| 3 | `auditah.toml` with valid excludes (`vendor/**`, `*.bak`) → `Config::load` | `Ok`, excludes preserved |
| 4 | `build_excludes` (audit) with invalid pattern | `Err(AuditError)` |
| 5 | `build_excludes` (credits) with invalid pattern | `Err(CreditsError)` |
| 6 | `render_record` with `Derivatives::Disallowed` override | round-trips as `"disallowed"` |
| 7 | `render_record` with `Derivatives::Allowed` override | round-trips as `"allowed"` |
| 8 | `render_record` with **all 7** override fields set | round-trips field-for-field |
| 9 | Sidecar `overrides.derivatives = "foobar"` → `resolve` | `Err` (invalid enum value) |
| 10 | Sidecar `[overrides] allows_modifications = true` → `resolve` | `Err` (unknown field, `deny_unknown_fields`) |

---

## Dialectical Outcomes (Why)

### Why `Result` instead of "make `build_excludes` infallible after validation"
The `Config` struct is `#[derive(Deserialize)]` and populated by `toml::from_str` in `Config::load`. A compiled `ExcludeMatcher`/`GlobSet` is **not serializable** and cannot live inside the same struct serde fills. Pre-storing a validated matcher would force a raw+validated split (overengineering). Worse, `Config` can be constructed **without** going through `Config::load` — tests do exactly this (`Config { exclude: vec![...], .. }`). An "infallible `build_excludes` that trusts load-validated globs" would rest on an unenforceable invariant. Returning `Result` makes `build_excludes` correct *regardless* of how `Config` was built. **Rejected alternative:** remove the `expect` and trust eager validation alone — unsafe for the direct-construction reason above.

### Why eager validation in `Config::load` *in addition to* fallible `build_excludes` (chosen A1)
Since `build_excludes` now returns `Result`, the panic is gone either way. Eager validation is a pure **fail-fast UX** layer: a bad glob in `auditah.toml` is a config-file error and should be reported at config load, not mid-walk. Cost: globs compile twice (once to validate at load, once to build) — microseconds for a handful of patterns, negligible. **Rejected alternative (A2):** drop eager validation, let `build_excludes` be the sole validator — simpler but a worse, later, less-pointed error message.

### Why `#[allow(...)]` per test module rather than `#![allow]` at crate root
Restriction lints fire on test code too, and the project enforces zero-warning CI. The existing lint config lives in `Cargo.toml` `[lints.clippy]` (`pedantic = warn`). To keep production honest while not churning every test, allow at the narrowest scope: `#[allow(clippy::unwrap_used, clippy::expect_used)]` on each `#[cfg(test)] mod tests` and `#![allow(...)]` atop each `tests/*.rs` file. **Rejected:** crate-root allow — would silently permit production panics.

### Why test the resolver path for `deny_unknown_fields`, not just unit-level
The "unrepresentable states" guarantee from the prior redesign is only real if it holds through the **actual parse path** a sidecar takes (`resolver::read_attribution` → `AttributionRecord` → embedded `Overrides`). The existing stale-key rejection tests live at the `LicenseTerms` unit level (`src/model/terms.rs`); they do not prove a malformed sidecar is rejected end-to-end. The new test (#10) closes that.

---

## Relevant Files (Where)

**Modified:**
- `src/config.rs` — add glob validation in `Config::load`; new `ConfigError`-producing helper.
- `src/audit.rs` — `build_excludes` returns `Result`; call site propagates with `?`.
- `src/credits.rs` — `build_excludes` returns `Result`; call site propagates with `?`.
- `Cargo.toml` — add the two restriction lints under `[lints.clippy]`.
- `src/test_support.rs` — add `#![allow(clippy::unwrap_used, clippy::expect_used)]` (feature-gated file).
- Every `src/**.rs` containing `#[cfg(test)] mod tests` — add `#[allow(...)]` on the test mod.
- Every `tests/*.rs` and `tests/common/mod.rs` — add `#![allow(...)]` at file top.

**New tests added to existing files:**
- `src/config.rs` — invalid-exclude `Config::load` test (case 1); valid-exclude-preserved test (case 3).
- `src/audit.rs` — `build_excludes` Err test (case 4) OR cover via `run_audit` with a directly-constructed bad Config (case 2).
- `src/credits.rs` — `build_excludes` Err test (case 5).
- `src/add.rs` — `derivatives_to_kebab` Disallowed/Allowed round-trip (cases 6, 7); full-7-field override round-trip (case 8).
- `tests/error_scenarios.rs` — invalid `derivatives` value (case 9); unknown override field (case 10).

---

## Key Code Context (What)

### The panic sites (production)
```rust
// src/audit.rs:77
fn build_excludes(ctx: &AuditCtx) -> ExcludeMatcher {
    let patterns = crate::discovery::all_excludes(&ctx.config.exclude);
    ExcludeMatcher::new(&patterns)
        .expect("default + user exclude patterns must compile (validated at config load)")
}

// src/credits.rs:80
fn build_excludes(ctx: &CreditsCtx) -> ExcludeMatcher {
    let patterns = crate::discovery::all_excludes(&ctx.config.exclude);
    ExcludeMatcher::new(&patterns).expect("exclude patterns must compile")
}
```

### `ExcludeMatcher::new` is already fallible
```rust
// src/discovery/enumerator.rs:29
pub fn new(patterns: &[String]) -> Result<Self, Report<EnumerateError>> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let glob = Glob::new(p).change_context(EnumerateError).attach(p.clone())?;
        builder.add(glob);
    }
    let set = builder.build().change_context(EnumerateError)
        .attach("failed to compile exclude glob set")?;
    Ok(Self { set })
}
```

### `Config` is serde-filled (cannot hold a compiled matcher)
```rust
// src/config.rs:15
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)] pub commercial_project: bool,
    #[serde(default)] pub redistributes_assets: bool,
    #[serde(default)] pub manual_review_acknowledged: Vec<String>,
    #[serde(default)] pub exclude: Vec<String>,
}

// src/config.rs:47
pub fn load(fs: &FsService, root: &std::path::Path) -> Result<Self, Report<ConfigError>> {
    let path = root.join(CONFIG_FILENAME);
    if !fs.exists(&path) { return Ok(Self::default()); }
    let content = fs.read_to_string(&path)
        .change_context(ConfigError).attach("failed to read project config")?;
    toml::from_str(&content)
        .change_context(ConfigError)
        .attach("failed to parse auditah.toml")
        .attach(path.display().to_string())
    // ← NO glob validation here today
}
```

### Call sites that must propagate the new `Result`
```rust
// src/audit.rs:44  run_audit
pub fn run_audit(ctx: &AuditCtx) -> Result<AuditReport, Report<AuditError>> {
    let excludes = build_excludes(ctx);   // becomes: build_excludes(ctx)?
    let assets = enumerate(&ctx.services.fs, ctx.root, &excludes)...

// src/credits.rs:51  collect_credits
pub(crate) fn collect_credits(ctx: &CreditsCtx)
    -> Result<BTreeMap<String, Vec<CreditEntry>>, Report<CreditsError>> {
    let excludes = build_excludes(ctx);   // becomes: build_excludes(ctx)?
    let assets = enumerate(&ctx.services.fs, ctx.root, &excludes)...
```

### Current lint config
```toml
# Cargo.toml:39
[lints.clippy]
pedantic = { level = "warn", priority = -1 }
```

### Override/table construction under test (src/add.rs)
```rust
fn has_any_override(o: &Overrides) -> bool { /* 7-field OR chain */ }
fn derivatives_to_kebab(d: &Overrides) -> Option<&'static str> {
    d.derivatives.as_ref().map(|d| match d {
        Derivatives::Disallowed => "disallowed",
        Derivatives::Allowed => "allowed",
        Derivatives::ShareAlike => "share-alike",
    })
}
fn override_table(o: &Overrides) -> toml_edit::Item { /* per-field value() emissions */ }
```
Only `ShareAlike` is currently exercised by a round-trip test; `Disallowed`/`Allowed` branches and the full-table path are untested.

---

## Implementation Algorithm (How)

### Phase 1 — Eliminate the panics (config + build_excludes)
1. In `src/config.rs`, add a private helper, e.g. `fn validate_excludes(exclude: &[String]) -> Result<(), Report<ConfigError>>` that calls `crate::discovery::all_excludes(exclude)` then `ExcludeMatcher::new(&patterns).change_context(ConfigError)` and discards the `Ok`. (Dependency: config → discovery + discovery/enumerator. No cycle: discovery does not import config.)
2. In `Config::load`, after the successful `toml::from_str`, call `validate_excludes(&cfg.exclude)?` before returning `Ok(cfg)`. The default-config path (`Ok(Self::default())`) needs no validation (empty excludes always compile).
3. In `src/audit.rs`, change `fn build_excludes(ctx: &AuditCtx) -> Result<ExcludeMatcher, Report<AuditError>>` returning `ExcludeMatcher::new(&patterns).change_context(AuditError).attach("invalid exclude glob in auditah.toml")`. Call site: `let excludes = build_excludes(ctx)?;`.
4. In `src/credits.rs`, mirror: `fn build_excludes(ctx: &CreditsCtx) -> Result<ExcludeMatcher, Report<CreditsError>>`; call site `let excludes = build_excludes(ctx)?;`.
5. Build; `run_audit`/`collect_credits` already return `Result<_, Report<_>>`, so `?` propagates with no signature change upstream.

### Phase 2 — Enable restriction lints and scope them out of tests
1. In `Cargo.toml` `[lints.clippy]`, add:
   ```toml
   unwrap_used = { level = "warn", priority = -1 }
   expect_used = { level = "warn", priority = -1 }
   ```
2. Add `#[allow(clippy::unwrap_used, clippy::expect_used)]` to each `#[cfg(test)] mod tests` in `src/` (14 files: `init_licenses`, `add`, `services/fs`, `test_support`, `services`, `config`, `audit/report`, `discovery`, `registry`, `discovery/resolver`, `discovery/enumerator`, `model/attribution`, `model/license`, `model/terms`).
3. For `src/test_support.rs` (gated `#![cfg(feature = "test-helper")]`), add `#![allow(clippy::unwrap_used, clippy::expect_used)]` as a second inner attribute at the top.
4. Add `#![allow(clippy::unwrap_used, clippy::expect_used)]` to the top of each `tests/*.rs` file (7 files) and `tests/common/mod.rs`.
5. Run `cargo clippy --tests`; fix any remaining production warnings (there should be none after Phase 1).

### Phase 3 — Override/table-construction tests (Check 2)
1. In `src/add.rs` tests: add `rendered_record_with_disallowed_derivatives_round_trips` and `..._allowed_...` (cases 6, 7), modeled on the existing ShareAlike test.
2. Add `rendered_record_with_all_override_fields_round_trips` (case 8): construct an `Overrides` with every field set to a non-default value, render, parse back, `assert_eq!` field-for-field.
3. In `tests/error_scenarios.rs`: add `resolve_errors_on_sidecar_with_invalid_derivatives_value` (case 9) — sidecar TOML with `derivatives = "foobar"` under `[overrides]`; assert `resolve(...).is_err()`.
4. Add `resolve_errors_on_sidecar_with_unknown_override_field` (case 10) — sidecar with `[overrides]\nallows_modifications = true` (the stale, removed key); assert `resolve(...).is_err()` (proves `deny_unknown_fields` on `Overrides` through the resolver).

---

## Anti-Goals (Out of Scope)

- **No new data model fields.** This task touches only validation/error-propagation/lints/tests. Do not alter `LicenseTerms`, `Overrides`, `Derivatives`, or `Config` fields.
- **No splitting `Config` into raw+validated forms.** Validation is a side-effecting check in `load`, not a type-level transformation.
- **No replacing `unwrap`/`expect` in test code with `?`.** Test code keeps them; they are `allow`-ed, not removed.
- **No changes to the audit/credits *finding* logic.** Only the `build_excludes` error path changes.
- **No new external dependencies.** `globset` is already a dependency.

---

## Edge Cases & Gotchas

- **Globs compile twice** (once in `Config::load` to validate, once in `build_excludes` to build). Accepted trade-off — negligible cost for a handful of patterns. Do not "optimize" by caching the matcher (would require the raw+validated split that is explicitly out of scope).
- **`Config::default()` path** returns early without validation. This is correct — default excludes are empty and always compile. Do not validate the default path.
- **Direct `Config` construction in tests** bypasses `load`, so eager validation does *not* protect those. This is exactly why `build_excludes` *also* returns `Result` — defense in depth. Tests that construct `Config` with a bad exclude must still hit `Err` at `build_excludes`/`run_audit`, not panic.
- **`expect_used` also fires on `assert!.unwrap()`-style helpers** — the per-mod `allow` covers these. Verify no production file-level helper uses `expect`.
- **`priority = -1`** on the new lint entries matches `pedantic`'s priority so they compose correctly with group lints.
- **`tests/common/mod.rs`** is `#![allow(dead_code)]` already; the clippy allow is additive and does not conflict.

---

## Navigation Anchors

- **`Config::load`** (`src/config.rs:47`) — primary entry point for eager validation.
- **`build_excludes`** (`src/audit.rs:77`, `src/credits.rs:80`) — the two panic sites to make fallible.
- **`ExcludeMatcher::new`** (`src/discovery/enumerator.rs:29`) — the underlying fallible constructor; reused unchanged.
- **`run_audit`** (`src/audit.rs:44`) / **`collect_credits`** (`src/credits.rs:51`) — propagate the new `?`.
- **`render_record` / `override_table` / `derivatives_to_kebab`** (`src/add.rs:26/119/110`) — targets of the construction tests.
- **`resolve` / `read_attribution`** (`src/discovery/resolver.rs:81/122`) — targets of the resolver failure tests.

---

## Dependency Mappings

- **`globset` (already `= "0.4"` in `Cargo.toml`)** — used by `ExcludeMatcher::new`; reused for validation. No version change.
- **`crate::discovery::all_excludes`** (`src/discovery.rs:47`) — produces the merged default+user glob list; called from both `build_excludes` and the new `validate_excludes`.
- **`crate::discovery::enumerator::{ExcludeMatcher, EnumerateError}`** — `ExcludeMatcher::new` returns `Report<EnumerateError>`; converted to `ConfigError`/`AuditError`/`CreditsError` via `.change_context`.
- **No new external crates.** The lints are built-in clippy restrictions.

---

## Test Strategies

- **Phase 1 (config/build_excludes):** add a unit test in `src/config.rs` that loads an `auditah.toml` with `exclude = ["**/[invalid"]` and asserts `Err`; add one with valid globs asserting `Ok` + preserved vec. Cover `build_excludes` failure via `run_audit`/`collect_credits` with a directly-constructed bad `Config` (asserts `Err`, *not* panic). Optionally a focused unit test calling `build_excludes` directly with a bad pattern.
- **Phase 2 (lints):** the test is `cargo clippy --tests` itself — must exit 0 with zero warnings. No new runtime tests.
- **Phase 3 (construction):** three BDD-style unit tests in `src/add.rs` (Disallowed, Allowed, full-table) mirroring the existing ShareAlike round-trip; two integration tests in `tests/error_scenarios.rs` (invalid derivatives value, unknown override field) using `FakeFs` + `resolve`, asserting `.is_err()`.
- **Verification phase:** re-run full `cargo nextest run`, `cargo clippy --tests`, `cargo fmt --check`; grep `src/` to confirm no production `unwrap`/`expect` remain outside `#[cfg(test)]` / `test_support` / `tests/`.
