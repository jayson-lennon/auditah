# auditah — Test Conformance & Error-Coverage Overhaul

## Problem

The test suite (70 tests) does not conform to the rust-programming skill's testing patterns, error-handling is undertested, and the recently-refactored CLI lost its compliance-failure exit-code signal:

- **Zero** tests use BDD structure (`// Given/When/Then`) — skill §2 violation across 100% of tests.
- **7 divergent copies** of `FakeFs` with *conflicting* `list_dir`/`walk` implementations — latent correctness risk.
- **~13 duplicated helpers** (`services()`, `seed_licenses()`, `codes_for()`, etc.) copy-pasted across integration files.
- **Multi-concept tests** cram separate behaviors into one `#[test]` — skill §1 violation (e.g. `cc_by_requires_attribution_but_cc0_does_not`).
- **Near-zero error-scenario coverage** despite a `Result`/`Report`-heavy codebase.
- **CLI regression:** the blanket `run() -> Result<(), Report<AppError>>` refactor means `audit` now exits `0` even when FAIL findings are present — a compliance tool that passes CI on violations.
- **`RealFs::walk` swallows all errors** — a walk over a missing root silently returns empty, masking real problems.

## Solution

1. **Shared test infra** behind a `test-helper` Cargo feature: one faithful `FakeFs` (`#[doc(hidden)] pub`), population helpers (`with_files`/`insert`), and per-operation IO-error injection (`fail_read`/`fail_write`/`fail_walk`/`fail_list_dir`). Plus `tests/common/mod.rs` for integration-only dedup.
2. **Strict BDD comments** on every `#[test]`: `// Given`, `// When`, `// Then` (`// And` only elaborating same concept). Missing = failure.
3. **Maximally aggressive splitting** — one concept per test.
4. **`rstest` expansion** for same-property obligation-check families.
5. **Error-scenario coverage** for every `Result`-returning public fn — content errors via FakeFs bad-content, IO errors via hybrid approach (FakeFs injection for unit tests, `temptree` + structurally-unwritable path for integration).
6. **CLI semantics fix:** move `cli/` into the lib (`auditah::cli`); shared `Result<CommandStatus, Report<AppError>>` where `CommandStatus::{Success, ComplianceFailure}`; `Err` = technical failure only. `main` maps `Ok(Success)→0`, `Ok(ComplianceFailure)→1`, `Err→2`.
7. **`RealFs::walk` fix:** propagate root-level walk errors instead of `filter_map`-ing them away; keep skipping individual *entry* errors.

---

## Dialectical Outcomes (Why)

### Why a `test-helper` Cargo feature (not `pub(crate)` or `cfg(test)`)
- **`pub(crate)` + `#[cfg(test)]`** dedupes the 7 unit-side `FakeFs` copies, but **physically cannot** dedupe the integration helpers in `tests/*.rs`. Integration tests are a separate compiled crate that links the library as an external dependency; `cfg(test)` items are compiled out of the non-test library build, and `pub(crate)` items are invisible externally.
- **`pub` + `#[doc(hidden)]` behind a `test-helper` feature** is the standard Rust pattern: the test infra is `pub` (reachable from integration tests via `auditah::test_support::FakeFs`) and `#[doc(hidden)]` (excluded from generated docs), gated behind a feature that's off in default builds. Normal `cargo build` produces no test machinery.
- The user updated the skill to clarify that the "per-feature test module" rule is about **domain builders** (e.g. `SessionBuilder`), not generic infra like an in-memory FS fake. Sharing `FakeFs` does not violate the skill.

### Why `CommandStatus` (not per-command return types, not an error for findings)
- **Per-command return types** (audit returns `Result<AuditOutcome>`, others `Result<()>`) makes the `run()` signatures non-uniform and forces `main` into per-command match arms.
- **A `ComplianceFailure` error variant** contradicts the agreed semantic model: `Err` = technical failure (the command couldn't complete its work); `Ok` = the work completed, including when `audit` ran fine but found violations. Violations are a *result*, not a crash.
- **Shared `Result<CommandStatus, Report<AppError>>`** honors the semantic model exactly, keeps signatures uniform, and `main` becomes a trivial 3-way match. `ComplianceFailure` being audit-only is self-documenting via the type.

### Why hybrid IO-error testing (FakeFs injection + temptree)
- **5 of 10 error tests are content errors** (malformed TOML, missing field, empty text) — the FakeFs just serves bad file contents; the function reads fine then fails to parse. No injection mechanism needed beyond `insert`/`with_files`.
- **5 of 10 are IO errors** (write denied, walk fails). FakeFs injection (`fail_write`) is fast and in-memory; `temptree` + a structurally-unwritable path (writing under an existing file) exercises the real `RealFs → FsError` chain.
- **Hybrid** matches the existing test topology: unit tests that already use FakeFs keep using it; integration tests that already use `RealFs` + `temptree` keep using that. No backend switch forced.

### Why fix `RealFs::walk` (W1)
- The current impl `filter_map`s away *all* walk errors, so a walk over a missing root silently returns empty — `run_audit` would report zero assets, masking a real misconfiguration.
- **Decision:** propagate *root-level* walk errors (return `Err`); keep skipping *individual entry* errors (a single unreadable file shouldn't abort the whole walk).

### Rejected alternatives
- **Subprocess CLI testing:** slow, brittle; unnecessary once `run()` returns `Result<CommandStatus>` and `cli/` is in the lib (directly callable).
- **B-only (temptree unwritable paths):** doesn't serve unit tests that are already FakeFs-backed.
- **W2 (document walk as infallible):** leaves the `Result` vestigial and masks misconfiguration.

---

## Relevant Files (Where)

### Created
- `src/test_support.rs` — shared `FakeFs` + population + IO-error injection. `#[cfg(feature = "test-helper")] #[doc(hidden)] pub`.
- `tests/common/mod.rs` — shared integration helpers (`services()`, `seed_licenses()`, `codes_for()`, config builders, `record()`).

### Modified
- `Cargo.toml` — add `[features] test-helper = []`.
- `src/lib.rs` — wire `pub mod cli;` and `#[cfg(feature = "test-helper")] pub mod test_support;`.
- `src/cli/*.rs` (move from bin crate) — `src/cli/` becomes `auditah::cli`; rewrite all `run()` to `Result<CommandStatus, Report<AppError>>`; audit returns `Ok(ComplianceFailure)` on FAIL findings.
- `src/main.rs` — slim to: parse CLI, call `cli::*::run`, map `CommandStatus`/`Err` → exit code.
- `src/services/fs.rs` — fix `RealFs::walk` to propagate root errors.
- All `src/*.rs` inline `#[cfg(test)]` modules — delete inline `FakeFs`, use `crate::test_support::FakeFs`; convert to BDD; split multi-concept.
- All `tests/*.rs` — delete duplicated helpers, use `tests/common/mod.rs`; convert to BDD; split multi-concept.
- `src/audit.rs` — no logic change; the audit command layer reads `report.has_failures()` to pick `ComplianceFailure`.

### Key paths to know
- `src/services/fs.rs` — `FsBackend` trait, `FsService` wrapper, `RealFs`.
- `src/audit.rs` — `run_audit`, `AuditCtx`, obligation checks.
- `src/audit/report.rs` — `AuditReport`, `Finding`, `FindingCode`, `Severity`.
- `src/model/terms.rs` — `LicenseTerms`, `Overrides`, `effective_terms`.
- `src/registry.rs` — `LicenseRegistry::load` (project-local merge).
- `src/discovery/enumerator.rs` — `ExcludeMatcher::new`.

---

## Key Code Context (What)

### FsBackend trait (the surface FakeFs must implement faithfully)
```rust
// src/services/fs.rs
pub trait FsBackend: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>>;
    fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>>;
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;
    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;
    fn exists(&self, &Path) -> bool;
    fn name(&self) -> &'static str;
}
```
**Current `RealFs::walk` (the bug — fix in Phase 3):**
```rust
fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
    Ok(walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(std::result::Result::ok)   // ← swallows root errors
        .filter(|e| e.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .collect())
}
```
**Faithful FakeFs must implement** (none of the 7 copies do this coherently):
- `list_dir(p)` = immediate children of `p` (files + dirs), parent-relative.
- `walk(root)` = all files recursively under `root`.

### Services container
```rust
// src/services.rs (src/services/mod.rs)
pub struct Services {
    pub fs: FsService,
    pub registry: LicenseRegistry,
}
impl Services {
    pub fn real() -> Result<Self, Report<ServicesError>>;
    pub fn from_parts(fs: FsService, registry: LicenseRegistry) -> Self;
}
```

### Audit pipeline entry + report
```rust
// src/audit.rs
pub struct AuditCtx<'a> { pub services: &'a Services, pub config: &'a Config, pub root: &'a Path }
pub fn run_audit(ctx: &AuditCtx) -> Result<AuditReport, Report<AuditError>>;
// src/audit/report.rs
pub struct AuditReport { pub findings: Vec<Finding> }
impl AuditReport {
    pub fn has_failures(&self) -> bool;   // ← audit command reads this for ComplianceFailure
    pub fn fail_count(&self) -> usize;
    pub fn flag_count(&self) -> usize;
}
```

### CLI run() — current (to be replaced)
```rust
// src/cli/audit_cmd.rs (CURRENT — all 5 commands)
pub fn run(cmd: &AuditCmd) -> Result<(), Report<AppError>> { ... }
```
**Target:** `pub fn run(cmd: &AuditCmd) -> Result<CommandStatus, Report<AppError>>` where audit returns `Ok(ComplianceFailure)` when `report.has_failures()`.

### Obligation checks (rstest-parameterization targets)
```rust
// src/audit.rs — each maps a term violation → FindingCode
check_coverage     → UnlicensedAsset (no sidecar/manifest)
check_resolution   → UnknownLicense (id not in registry)
check_license_text → MissingLicenseText (no LICENSES/<id>.txt)
check_obligations:
  requires_attribution (missing title/author/source) → IncompleteAttribution
  allows_commercial_use=false + commercial_project=true → CommercialViolation
  allows_modifications=false + modified=true → NoDerivativesModification
  requires_share_alike / requires_source_disclosure / requires_license_notice → FLAG (ShareAlikeReview / SourceDisclosureReview / LicenseNoticeReview)
```

### Config + registry load (error-test targets)
```rust
// src/config.rs
pub const CONFIG_FILENAME: &str = "auditah.toml";
pub fn load(fs: &FsService, root: &Path) -> Result<Self, Report<ConfigError>>;
// src/registry.rs
pub fn load(fs: &FsService, project_root: &Path) -> Result<Self, Report<RegistryError>>;
// src/discovery/enumerator.rs
pub fn new(patterns: &[String]) -> Result<Self, Report<EnumerateError>>;
```

---

## Implementation Algorithm (How)

### Phase 1 — Test infra
1. Add `[features] test-helper = []` to `Cargo.toml`. (Empty feature — gates module via `#[cfg(feature)]`.)
2. Create `src/test_support.rs`:
   - `pub struct FakeFs { files: Mutex<HashMap<PathBuf, String>>, fail_reads: ..., fail_writes: ..., fail_walk_roots: ..., fail_list_dirs: ... }` (use `HashSet<PathBuf>` for each failure set).
   - `FakeFs::with_files([(path, content)])` constructor + builder methods `insert(path, content)`, `fail_read(path)`, `fail_write(path)`, `fail_walk(root)`, `fail_list_dir(path)`.
   - Implement `FsBackend` **faithfully**: `list_dir` = immediate children (filter keys by `parent() == Some(p)`); `walk` = recursive files under root (filter keys by `starts_with(root)`); failure sets short-circuit to `Err(FsError)` before lookup.
3. Register in `src/lib.rs`: `#[cfg(feature = "test-helper")] #[doc(hidden)] pub mod test_support;`.
4. Create `tests/common/mod.rs` with the deduped helpers. Each `tests/*.rs` does `mod common; use common::*;`.
5. Delete all 7 inline `FakeFs` definitions; replace with `use crate::test_support::FakeFs`.
6. Delete all duplicated integration helpers; route through `tests/common/mod.rs`.
7. **Expect fallout:** the faithful `FakeFs` will surface tests that previously passed because their copy returned `Vec::new()` for `list_dir`/`walk`. Fix each as a test-correctness issue (adjust expected counts or seed the files the test actually queries).

### Phase 2 — CLI refactor
1. Move `src/cli/` content into the library: add `pub mod cli;` to `src/lib.rs`; the modules become `auditah::cli::{audit_cmd, credits_cmd, add_cmd, init_licenses_cmd, init_pack_cmd}`.
2. Define `CommandStatus` in `src/lib.rs` (or a `src/cli/mod.rs`):
   ```rust
   pub enum CommandStatus { Success, ComplianceFailure }
   ```
3. Rewrite each `run()` to `Result<CommandStatus, Report<AppError>>`:
   - audit: run pipeline; `render_report`; if `report.has_failures()` → `Ok(ComplianceFailure)` else `Ok(Success)`.
   - others: unchanged except return `Ok(CommandStatus::Success)`.
4. Rewrite `main`: parse → match command → call `run()` → map result to exit code via a `fn to_exit_code(result: Result<CommandStatus, Report<AppError>>) -> i32`:
   - `Ok(Success) → 0`, `Ok(ComplianceFailure) → 1`, `Err → 2` (print report via `{:?}`).
5. **In dev-dependencies**, ensure `test-helper` feature is enabled so integration tests can call `auditah::cli::*::run` (note: integration tests already link the lib; the cli modules are `pub` in the lib, so feature-gating test_support is independent).

### Phase 3 — `RealFs::walk` fix
1. Replace the `filter_map(Result::ok)` with: check the *first* iterator entry; if it's an `Err`, return `Err(FsError)`. Otherwise collect, skipping subsequent `Err`s (per-entry robustness).
   ```rust
   let mut iter = WalkDir::new(root).into_iter();
   // root error propagates
   match iter.next() {
       Some(Err(_)) => return Err(Report::new(FsError)).attach(...),
       None => return Ok(vec![]),
       Some(Ok(_)) => {}
   }
   ```
2. Collect remaining: `filter_map(Result::ok)` for entries, `.filter(is_file)`, `.map(into_path)`.

### Phases 4–5 — BDD + split
- For each existing `#[test]`: add `// Given`, `// When`, `// Then` comments in order.
- Split any test with multiple `// When`/`// Then` blocks or multiple distinct concepts into separate tests (one behavior each). Example: `cc_by_requires_attribution_but_cc0_does_not` → `cc_by_requires_attribution` + `cc0_does_not_require_attribution`.
- Round-trip equality tests (`assert_eq!(parsed, original)`) are **one concept** — keep intact.
- Keep BDD comment ordering tight: Given (setup) → When (action) → Then (assertion). `// And` only elaborates the same concept.

### Phase 6 — rstest expansion
- The obligation family is the same property: *"a term violation produces a specific finding code"*. Parameterize:
  - `#[case]` rows for each (term-state, expected FindingCode): missing coverage→UnlicensedAsset, NC+commercial→CommercialViolation, no-derivs+modified→NoDerivativesModification, missing attribution→IncompleteAttribution.
- Registry known-id lookup (existing rstest) retained.

### Phase 7 — Error-scenario coverage
- **Content errors** (FakeFs serves bad content via `with_files`/`insert`):
  - `LicenseRegistry::load`: malformed `licenses/*.toml`; `LicenseRef-*` with `text = ""`.
  - `ExcludeMatcher::new`: invalid glob string (no FsBackend needed).
  - `resolve`: malformed sidecar TOML; sidecar missing `license` field.
- **IO errors** (hybrid):
  - *FakeFs injection (unit):* `write_sidecar`, `generate_credits`, `init_licenses` — register `fail_write` on the output path → assert `Err`.
  - *temptree integration:* `write_sidecar` to a path under an existing file → real `FsError`.
  - *walk failure:* `run_audit` with `FakeFs.fail_walk(root)` → `Err(AuditError)`.
- Each test: assert `is_err()` and the error **type** (`RegistryError`/`EnumerateError`/`ResolveError`/`AuditError`/`AddError`/`CreditsError`/`InitLicensesError`). Optionally assert attached context where meaningful.
- CLI error tests: `auditah::cli::add_cmd::run(...)` with write failure → `Err`; clean → `Ok(Success)`; violations → `Ok(ComplianceFailure)`.

---

## Anti-Goals (Out of Scope)

- **No new audit/credits logic.** The audit obligation engine, finding codes, and credits emission are correct; only their *tests* change (plus the walk-propagation bug fix and CLI return-type semantics).
- **No subprocess CLI testing.** Once `run()` returns `CommandStatus`, all CLI behavior is tested by direct call.
- **No RealFs `list_dir`/`read_to_string`/`write` error injection.** Only `walk` root-error propagation is fixed (W1); other RealFs methods unchanged.
- **No change to the on-disk attribution format** (`.attr.toml` / `manifest.toml` / `LICENSES/`).
- **No change to audit *semantics*** (which terms fail vs flag). Only the *command* layer's exit-code signal changes.
- **No public API for `test_support` consumers beyond the feature gate.** It's `#[doc(hidden)]`; not part of the supported API.
- **No per-struct test builders** moved to shared modules (the skill's builder rule applies to domain builders, which stay per-feature).
- **No new dependencies.** `rstest`, `temptree`, `parking_lot` already in dev-deps.

---

## Edge Cases & Gotchas

1. **Faithful FakeFs surfaces hidden test bugs.** Several inline fakes returned `Vec::new()` for `list_dir`/`walk`, so tests querying children passed vacuously. The faithful version returns real children — some tests will fail and need their expected values corrected (more files discovered, orphan detection finding real orphans, etc.). Treat as test-correctness fixes, not regressions.
2. **`temptree` nested-dir syntax.** Nested directories in `temptree!` use block sub-trees: `"sub": { "manifest.toml": "..." }`, **not** `"sub/manifest.toml": "..."` (the latter throws "No such file or directory" at tree construction). Already bitten once; re-verify any new temptree with nested paths.
3. **`main` return type.** `fn main() -> Result<(), E>` causes Rust to print the error and exit **1** on `Err`. Since we need exit **2** for technical failures, `main` must not propagate the `Result` directly — it must call `std::process::exit(to_exit_code(result))`. Don't let `main` return `Result`.
4. **`test-helper` feature off by default.** Integration tests enable it via dev-dependency feature passthrough or a `[dev-dependencies] auditah = { features = ["test-helper"] }` self-reference. Verify default `cargo build` produces no `test_support` symbol.
5. **`// And` discipline.** `// And` may only elaborate the *same* concept as the preceding `// Then`. A `// And` that asserts a different behavior is a hidden multi-concept test — split it.
6. **rstest same-property rule.** Don't cram different behaviors (validation vs success) into one `#[rstest]`. The obligation family is legitimately one property ("term violated → code"); malformed-input-vs-valid-input is **not** one property.
7. **Round-trip tests are one concept.** `assert_eq!(parsed, original)` checks "serialization round-trips" — keep as single tests even though it compares whole structs.
8. **`main` exit-code test.** Testing `fn main` directly is hard; extract the mapping into `pub fn to_exit_code(result: Result<CommandStatus, Report<AppError>>) -> i32` (or `pub fn dispatch(cli: Cli) -> i32`) and test that.
9. **IO-error injection granularity for walk.** `fail_walk(root)` operates on a *root*, not individual files — different granularity than file-level read/write injection. Document in the helper doc-comment.
10. **`Services::real()` is fallible now** (returns `Result`). The CLI already handles this via `.change_context(AppError)?`. Tests constructing `Services` use `from_parts` with `FakeFs`, avoiding `real()`.

---

## Navigation Anchors (Entry Points)

- **`src/services/fs.rs` — `FsBackend` trait + `RealFs`**: the contract FakeFs implements; the walk bug site.
- **`src/audit.rs` — `run_audit`**: the pipeline tests drive; reads `has_failures()` for the ComplianceFailure signal.
- **`src/audit/report.rs` — `AuditReport::has_failures()`**: the boolean that picks `ComplianceFailure` vs `Success`.
- **`src/cli/audit_cmd.rs` — `run()`**: becomes `Result<CommandStatus, …>`, the audit-only ComplianceFailure producer.
- **`src/main.rs` — `to_exit_code`/`dispatch`**: the 3-way mapping test target.
- **`src/test_support.rs` — `FakeFs`**: the single shared fake; all unit tests depend on it.
- **`tests/common/mod.rs`**: the single shared integration-helper module.
- **`src/discovery/enumerator.rs` — `ExcludeMatcher::new`**: glob-compile error target.
- **`src/registry.rs` — `LicenseRegistry::load`**: project-local merge error target.

---

## Dependency Mappings

### New internal modules
- `src/test_support.rs` → depends on `src/services/fs.rs` (`FsBackend`, `FsService`, `FsError`), `parking_lot::Mutex`, `std::collections::{HashMap, HashSet}`.
- `tests/common/mod.rs` → depends on `auditah::*` (lib), `temptree` (for helpers that build trees).
- `auditah::cli` (moved) → depends on `auditah::*` core modules.

### Cargo feature
- `test-helper = []` (empty feature; gates `src/test_support.rs` via `#[cfg(feature)]`).

### No new external crates
All required crates are already in `[dev-dependencies]`: `tempfile`, `temptree`, `rstest`, `parking_lot`.

---

## Test Strategies

### Phase 1 (infra) verification
- `grep -rn "struct FakeFs" src/ tests/` returns exactly **one** hit (`src/test_support.rs`).
- `grep -rn "^fn services()\|^fn seed_licenses()\|^fn codes_for()" tests/` returns zero hits (all in `tests/common/mod.rs`).
- `cargo build` (no feature) succeeds with no `test_support` symbol; `cargo build --features test-helper` compiles the module.
- Existing tests still pass (after fallout fixes).

### Phase 2 (CLI) verification
- Direct-call tests:
  - `auditah::cli::audit_cmd::run(clean_project) → Ok(CommandStatus::Success)`.
  - `auditah::cli::audit_cmd::run(project_with_fail_findings) → Ok(CommandStatus::ComplianceFailure)`.
  - `auditah::cli::audit_cmd::run(config_load_failure) → Err`.
  - `auditah::cli::add_cmd::run(unwritable_path) → Err`.
- Exit-code mapping: `to_exit_code` / `dispatch` test asserts `0/1/2`.

### Phase 3 (walk) verification
- `RealFs::walk` over `temptree` with missing subdir → `Err(FsError)`.
- `RealFs::walk` over tree with one unreadable entry → `Ok` with the rest (skip entry errors).

### Phases 4–5 (BDD + split) verification
- Grep: every `#[test]` has `// Given`, `// When`, `// Then` within ~10 lines above/around it.
- No `#[test]` body contains two `// When` or two `// Then` markers.
- Test count increases (splitting adds tests); confirm via `cargo test -- --list | grep -c '::'`.

### Phase 6 (rstest) verification
- Obligation family `#[rstest]` parameterizes (term-state, expected FindingCode) rows; each case is the same property.

### Phase 7 (error coverage) verification
- For each `Result`-returning public fn, there's an `is_err()` test. Cross-check against the error-surface table in the plan.
- CLI error tests use direct `run()` calls, not subprocess.

### Final verification (Phase 8)
- `cargo test` all pass.
- `cargo clippy --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.
- Grep proofs: one FakeFs; 100% BDD coverage; zero duplicate helpers.

---

## Acceptance Criteria

1. Exactly **one** `FakeFs` definition in the entire codebase.
2. Zero duplicated integration helpers — all in `tests/common/mod.rs`.
3. Every `#[test]` has `// Given`, `// When`, `// Then` comments (enforced via grep in verification).
4. No test asserts more than one distinct concept.
5. Every `Result`-returning public fn has at least one error-scenario test.
6. `rstest` used for all applicable same-property families.
7. `test-helper` feature is `#[doc(hidden)]` and off by default — normal builds exclude test infra.
8. `cli/` is part of the library (`auditah::cli`); every `run()` is directly callable from tests.
9. `CommandStatus` distinguishes clean (`Success`) from violations (`ComplianceFailure`); `Err` reserved for technical failures.
10. Exit codes: `0` clean, `1` audit violations, `2` technical failure — verified by direct `run()` tests + `main` mapping test.
11. `FakeFs` supports per-operation IO-error injection (`fail_read`/`fail_write`/`fail_walk`/`fail_list_dir`).
12. `RealFs::walk` returns `Err` on root walk failure; still skips individual entry errors.
13. All tests pass, clippy `-D warnings` clean, `cargo fmt --check` clean.

---

## Phases

1. **Test infra** — add `test-helper` feature; create `src/test_support.rs` (faithful `FakeFs` + `with_files`/`insert` + IO-error injection methods, `#[cfg(feature)] #[doc(hidden)] pub`); create `tests/common/mod.rs`; delete all 7 inline `FakeFs` + duplicated helpers; fix fallouts from faithful `FakeFs` surfacing hidden bugs.
2. **CLI refactor** — move `src/cli/` → `auditah::cli`; introduce `CommandStatus`; rewrite each `run()` to the shared signature; audit returns `Ok(ComplianceFailure)` on FAIL findings; rewrite `main` 3-way mapping.
3. **`RealFs::walk` fix** — propagate root walk errors; keep entry-error skipping; unit test that walk over missing root returns `Err`.
4. **BDD + split: unit tests** — convert all unit tests to strict Given/When/Then; split multi-concept tests one-behavior-per-test.
5. **BDD + split: integration tests** — convert all integration tests; split multi-concept; migrate to `tests/common/mod.rs`.
6. **`rstest` expansion** — parameterize obligation-check families (uncovered→UnlicensedAsset, NC-under-commercial→fail, no-derivs→fail, missing-attr→fail) as same-property rstests.
7. **Error-scenario coverage** — content errors (registry malformed/empty-text, exclude matcher invalid glob, resolve malformed/missing-field) via FakeFs bad-content; IO errors (write_sidecar, generate_credits, init_licenses, CLI add write, run_audit walk) via hybrid FakeFs-injection (unit) + temptree-unwritable (integration).
8. **Verify** — full test pass, clippy clean, fmt clean; grep-prove zero remaining duplicate `FakeFs`/helpers and 100% BDD coverage; assert the 3-way exit-code contract.
