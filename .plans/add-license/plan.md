# Context-Rich Specification: `add-license`, single `LICENSES/` dir, drop embedded licenses

## Problem

No way to create a license grid — users had to hand-author TOML with no schema
guidance. The registry also carries three warts:

1. **Directory split:** a lowercase `licenses/` (grids, read by
   `merge_project_local` at `registry.rs:348`) and an uppercase `LICENSES/`
   (text, checked by `audit.rs:129`). There was never supposed to be a split —
   only `LICENSES/`.
2. **Dual-form model:** `LicenseRef-*` (custom, validated) vs. embedded
   well-known SPDX ids (MIT, CC0, CC-BY, OFL — seeded at compile time with no
   real story for the well-known half).
3. **Dead `text` field:** `LicenseRegistryEntry.text` is parsed from TOML and
   required for `LicenseRef-*` (`registry.rs:387`), but nothing consumes its
   content after `init-licenses` is removed. It was never supposed to be inline.

The seed-only `init-licenses` command writes hardcoded SPDX text with no grid
and is redundant.

## Solution

1. **Consolidate to a single `LICENSES/`** — grids (`.toml`) and text (`.txt`)
   live together; delete the lowercase `licenses/` path.
2. **Drop embedded licenses + inline `text`** — remove `src/embedded_licenses/`,
   `embedded_entries()`/`embedded_only()`, the `text` field + its serde attr,
   and the `registry.rs:387` inline-text guard. `LicenseRegistry::load` merges
   project-local over an empty map.
3. **Add a registry builder** — `LicenseRegistry::builder()` with fluent
   `.license(spec)` chaining and an in-memory `.build()` (common case) plus
   `.commit(root)?` (writes `LICENSES/<id>.toml` + loads, for disk tests).
4. **Add `add-license <name>`** — non-interactive template generator writing
   `<root>/LICENSES/LicenseRef-<name>.toml`: permissive defaults, every field
   commented, header explaining `id`/`LicenseRef-`/`LICENSES/<id>.txt`.
   Refuses to overwrite; `--root` (default `.`). Prints what it wrote.
5. **Scrap `init-licenses`** — delete command + module + CLI wiring; reword the
   audit `MissingLicenseText` hint.

---

## Dialectical Outcomes (Why)

### 1. Collapse to `LicenseRef-*` only (drop embedded well-known licenses)

**Decision:** Remove all embedded licenses. `LicenseRegistry::load` starts from
an empty map. Every license — including MIT/CC0/OFL — must be created via
`add-license` as `LicenseRef-*` before it resolves.

**Reasoning:** The reuse design we based the app on has a poorly-constructed
dual-form: `LicenseRef-*` (validated, custom) vs. well-known SPDX ids (seeded).
If we have no story for integrating "well-known" licenses (and SPDX has
*hundreds*, which would be crazy to embed), the dual-form collapses to just
`LicenseRef-*`. Since `LicenseRef-*` necessarily goes through the entire
validation pipeline, it's a strict superset of what "well-known" needs — so the
future well-known mechanism is an *authoring* problem, not a pipeline problem.

**Trade-off:** This shifts the burden of authoring the grid onto the user
*until* a story for well-known licenses is decided. Accepted: the forcing
function is intentional.

**Rejected alternative:** Keep embedded `.toml` grids, drop only embedded `.txt`
text. Rejected because the dual-form is the wart, not just the text. Keeping the
grids re-introduces the entanglement.

### 2. Single `LICENSES/` directory (no lowercase `licenses/`)

**Decision:** Both `.toml` grids and `.txt` text live in `LICENSES/`. The
lowercase `licenses/` path is deleted.

**Reasoning:** There was never supposed to be a split. The split caused the
"entanglement" confusion: tests had a `services()` shortcut using
`embedded_only()` and a separate `seed_licenses()` writing `.txt`. Consolidating
to one directory (matching reuse/SPDX convention) makes the model coherent: one
place for everything about a license.

### 3. Drop the `text` field from `LicenseRegistryEntry`

**Decision:** Remove `pub text: String` and the `registry.rs:387` guard that
required it non-empty for `LicenseRef-*`.

**Reasoning:** After `init-licenses` is removed, nothing consumes the `text`
field's content. The `audit` check (`audit.rs:129-130`) only checks
`LICENSES/<id>.txt` *exists* on disk — it never opens the inline field. Keeping
a validated-but-unconsumed field is a wart; the user confirmed "there was never
supposed to be inline text."

### 4. `add-license` is a template generator, not an interactive wizard

**Decision:** `add-license <name>` writes a fully-commented template with
permissive defaults. Non-interactive. User edits the file afterward.

**Reasoning:** The inline comments *are* the documentation. This sidesteps the
entire interaction-model question (no need to list enum variants at a prompt).
Matches `cargo init` writing a commented `Cargo.toml`. The user must edit to
confirm the licensing — the default terms being permissive means an unedited
file represents "use however you want," but failing the audit
(`MissingLicenseText` until `LICENSES/<id>.txt` is created) forces the user to
actually engage.

**Rejected alternative:** Interactive prompt (like `add`). Rejected: verbose,
slower for repeat use, and re-introduces typo risk that a template-with-comments
eliminates.

### 5. Permissive defaults in the template

**Decision:** Template starts from the "use however you want" baseline:
`derivatives = "allowed"`, all `allows_*` true, all `requires_*`/`manual_review`
false. Each line commented. User restricts from there.

**Reasoning:** This is the bespoke-permissive license shape we established fits
the grid. Defaulting to it and prompting for restrictions is the honest
representation of how licenses get authored (start open, restrict as needed).

### 6. Registry builder for tests (per the Rust programming skill §7 Test Builders)

**Decision:** Add `LicenseRegistry::builder()` — fluent, in-memory `.build()` +
`.commit(root)?` for disk tests. Tests construct the registry directly; no
`services()`/`seed_licenses()` shortcuts.

**Reasoning:** The skill mandates domain-specific builders for complex setup,
preferring a shared test module. The `LicenseRegistry` backing field is just a
`HashMap` (`registry.rs:291-293`), so in-memory construction is free and trivial.
The `.commit(root)` method handles the rare disk case (testing `add-license`
output, `load`, or the audit text-existence check).

### 7. Defer the well-known-license fetch mechanism

**Decision:** No SPDX-data fetching in this task. Defer to a future task.

**Reasoning:** The grid (the unique value) is hand-authored regardless of whether
text is fetched. Building fetch infrastructure to save one `curl` has low ROI and
adds maintenance surface (SPDX data versioning, offline behavior, network failure
modes) that doesn't advance the obligation-grid value prop.

---

## Relevant Files (Where)

### To create
- `src/add_license.rs` — the template renderer (`render_license_template`).
- `src/cli/add_license_cmd.rs` — the CLI command (`AddLicenseCmd` + `run`).
- `src/registry_builder.rs` — `LicenseRegistryBuilder` + `LicenseSpec` (or
  colocated in `src/registry.rs`; see Phase 1 note).
- `tests/common/mod.rs` (modify) — term fixtures (`permissive_terms()` etc.) and
  any shared test helpers.

### To modify
- `src/registry.rs` — drop `embedded_entries()`/`embedded_only()`, the `text`
  guard, `include_str!`; rewrite `LicenseRegistry::load` to start from empty;
  add `builder()`; consolidate `merge_project_local` to read `LICENSES/`.
- `src/model/license.rs` — drop `text` field + serde attr from
  `LicenseRegistryEntry`; update the `cc0_entry()` test helper.
- `src/services.rs` — `Services::real` constructs `LicenseRegistry::load` (path
  stays `.`, but the dir it reads is now `LICENSES/`); the embedded round-trip
  test.
- `src/main.rs` — remove `InitLicenses` arm/import/dispatch; add `AddLicense`.
- `src/audit.rs` — reword the `MissingLicenseText` message (`:135`) to drop the
  `init-licenses` hint; update tests referencing SPDX ids.
- `src/discovery.rs` — the exclude pattern `**/licenses/**` becomes
  `**/LICENSES/**` (case-sensitive matters here).
- `src/add.rs`, `src/cli/add_cmd.rs`, `src/cli/init_pack_cmd.rs` — update
  `--license` doc comments / tests referencing SPDX ids.
- `src/discovery/resolver.rs`, `src/model/attribution.rs` — tests referencing
  SPDX ids.
- `tests/*.rs` (7 integration files) — migrate SPDX-id-dependent tests to the
  builder + `LicenseRef-*`.
- `Cargo.toml` — remove any `embedded_licenses` references if present.

### To delete
- `src/embedded_licenses/` — the entire directory (CC0-1.0, CC-BY-3.0, MIT,
  OFL-1.1 `.toml` + `.txt`).
- `src/init_licenses.rs` — the `init_licenses` module.
- `src/cli/init_licenses_cmd.rs` — the `init-licenses` CLI command.

---

## Key Code Context (What)

### `LicenseRegistryEntry` (current — `src/model/license.rs:11-27`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseRegistryEntry {
    /// SPDX license ID (`CC-BY-3.0`, `MIT`, `CC0-1.0`) or `LicenseRef-*` for custom.
    pub id: String,
    /// Human-readable license name.
    pub name: String,
    /// Canonical URL of the license.
    pub url: String,
    /// Full license text. Embedded licenses carry this at compile time.
    #[serde(default)]
    pub text: String,                                    // ← DROP THIS
    /// Obligations and permissions of this license.
    pub terms: LicenseTerms,
    /// Free-form notes, especially for bespoke/custom licenses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}
```

After change: `text` removed. The struct doc comment updates: no more "embedded
via `include_str!`"; all entries come from `LICENSES/*.toml`.

### `LicenseRegistry` (current — `src/registry.rs:290-338`)

```rust
#[derive(Debug, Clone)]
pub struct LicenseRegistry {
    entries: HashMap<String, LicenseRegistryEntry>,
}

impl LicenseRegistry {
    pub fn load(fs: &FsService, project_root: &Path) -> Result<Self, Report<RegistryError>> {
        let mut entries = embedded_entries();           // ← becomes empty map
        merge_project_local(fs, project_root, &mut entries)?;
        Ok(Self { entries })
    }

    #[must_use]
    pub fn embedded_only() -> Self {                     // ← DELETE
        Self { entries: embedded_entries() }
    }

    pub fn get(&self, id: &str) -> Option<&LicenseRegistryEntry> { ... }
    pub fn entries(&self) -> impl Iterator<Item = &LicenseRegistryEntry> { ... }
    pub fn len(&self) -> usize { ... }
    pub fn is_empty(&self) -> bool { ... }
}
```

After change:
- `load` starts from `HashMap::new()`.
- `embedded_only()` deleted.
- Add `pub fn builder() -> LicenseRegistryBuilder`.
- Add `pub fn empty() -> Self` for the trivial empty case (used by
  `Services::real` when no `LICENSES/` dir exists, and tests).

### `merge_project_local` (current — `src/registry.rs:343-358`)

```rust
fn merge_project_local(
    fs: &FsService,
    project_root: &Path,
    entries: &mut HashMap<String, LicenseRegistryEntry>,
) -> Result<(), Report<RegistryError>> {
    let local_dir = project_root.join("licenses");       // ← "LICENSES"
    if !fs.exists(&local_dir) {
        return Ok(());
    }
    let toml_paths = list_local_tomls(fs, &local_dir)?;
    for path in toml_paths {
        let entry = read_and_parse_local(fs, &path)?;
        entries.insert(entry.id.clone(), entry);
    }
    Ok(())
}
```

Change: `"licenses"` → `"LICENSES"` on line 348. Rename `local_dir` →
`licenses_dir` for clarity (optional).

### `read_and_parse_local` (current — `src/registry.rs:372-394`)

```rust
fn read_and_parse_local(...) -> Result<LicenseRegistryEntry, Report<RegistryError>> {
    // ... read + parse ...
    if entry.id.starts_with("LicenseRef-") && entry.text.trim().is_empty() {   // ← DELETE
        return Err(...);
    }
    Ok(entry)
}
```

Change: delete lines 385-392 (the inline-text guard).

### `Command` enum (current — `src/main.rs:24-36`)

```rust
#[derive(Debug, Subcommand)]
enum Command {
    Audit(AuditCmd),
    Credits(CreditsCmd),
    Add(AddCmd),
    InitLicenses(InitLicensesCmd),     // ← DELETE
    InitPack(InitPackCmd),
}
```

After change: `InitLicenses` arm removed; `AddLicense(AddLicenseCmd)` added
(before `InitPack`, after `Add`).

### `audit.rs` `MissingLicenseText` message (current — `src/audit.rs:135`)

```rust
"license {license_id:?} has no LICENSES/{license_id}.txt; run `auditah init-licenses`"
```

After change (drop the init-licenses hint, point at the file):

```rust
"license {license_id:?} has no LICENSES/{license_id}.txt; create it with the full license text"
```

### `AddCmd` pattern (reference — `src/cli/add_cmd.rs:18-39`)

The `add-license` command follows the same `clap::Args` struct shape, with a
positional `<name>` + `--root` flag (default `.`), mirroring `InitLicensesCmd`:

```rust
#[derive(Debug, Args)]
pub struct AddLicenseCmd {
    /// Name for the license (becomes the `LicenseRef-<name>` id).
    pub name: String,
    /// Project root to write `LICENSES/` into (defaults to the current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}
```

---

## Implementation Algorithm (How)

### Phase 1: Registry + directory consolidation

1. **Drop embedded licenses:** In `src/registry.rs`:
   - Delete the `embedded_entries()` function (lines ~22-69) and the
     `include_str!` calls.
   - Delete `embedded_only()`.
   - In `LicenseRegistry::load`, replace `embedded_entries()` with
     `HashMap::new()`.
   - Add `pub fn empty() -> Self { Self { entries: HashMap::new() } }`.
   - Add `pub fn builder() -> LicenseRegistryBuilder { LicenseRegistryBuilder::default() }`.

2. **Consolidate directory:** In `merge_project_local`, change
   `project_root.join("licenses")` → `project_root.join("LICENSES")`.

3. **Drop `text` field:** In `src/model/license.rs`, remove `pub text: String`
   + its `#[serde(default)]` attr. Update the doc comment. Update the
   `cc0_entry()` test helper to drop `text`. In `read_and_parse_local`, delete
   the inline-text guard (lines 385-392).

4. **Add the builder** (`src/registry.rs` or a new `src/registry_builder.rs`):

   ```rust
   #[derive(Debug, Clone, Default)]
   pub struct LicenseRegistryBuilder {
       specs: Vec<LicenseSpec>,
   }

   impl LicenseRegistryBuilder {
       pub fn license(mut self, spec: LicenseSpec) -> Self {
           self.specs.push(spec);
           self
       }
       /// In-memory construction — the common case. No disk touched.
       #[must_use]
       pub fn build(self) -> LicenseRegistry {
           let mut entries = HashMap::new();
           for spec in self.specs {
               entries.insert(spec.id.clone(), spec.into_entry());
           }
           LicenseRegistry { entries }
       }
       /// Write LICENSES/<id>.toml for each spec, then load the merged registry.
       /// For tests that need disk (add-license output, load, audit text-check).
       pub fn commit(self, root: &Path, fs: &FsService) -> Result<LicenseRegistry, Report<RegistryError>> {
           let dir = root.join("LICENSES");
           for spec in &self.specs {
               let path = dir.join(format!("{}.toml", spec.id));
               let toml = toml::to_string(&spec.into_entry())
                   .change_context(RegistryError)?;
               fs.write(&path, &toml).change_context(RegistryError)?;
           }
           LicenseRegistry::load(fs, root)
       }
   }

   pub struct LicenseSpec {
       id: String,
       entry: LicenseRegistryEntry,
   }

   impl LicenseSpec {
       pub fn new(id: &str) -> Self {
           Self {
               id: id.to_string(),
               entry: LicenseRegistryEntry {
                   id: id.to_string(),
                   name: id.to_string(),
                   url: String::new(),
                   terms: permissive_terms(),   // test fixture, defined in common/mod.rs or here
                   notes: None,
               },
           }
       }
       pub fn terms(mut self, terms: LicenseTerms) -> Self { self.entry.terms = terms; self }
       pub fn name(mut self, name: &str) -> Self { self.entry.name = name.to_string(); self }
       pub fn into_entry(self) -> LicenseRegistryEntry { self.entry }
   }
   ```

   **Note on `permissive_terms()` location:** The builder needs a default terms
   baseline. If the builder is in `src/`, `permissive_terms()` must be a real
   production fn (it's useful for `add-license`'s template too). Define it in
   `src/model/terms.rs` (e.g. `LicenseTerms::permissive()`) so both production
   and tests use it.

### Phase 2: `add-license` + scrap `init-licenses`

1. **Add `add_license.rs`:**

   ```rust
   pub fn render_license_template(name: &str) -> String {
       // Header comment explaining id/LicenseRef-/LICENSES/<id>.txt relationship.
       // id = "LicenseRef-<name>"
       // name = "TODO: human-readable name"
       // url = ""
       // [terms] with permissive defaults, each line preceded by a # comment.
   }

   pub fn write_license_template(
       services: &Services, root: &Path, name: &str,
   ) -> Result<PathBuf, Report<AddLicenseError>> {
       let path = root.join("LICENSES").join(format!("LicenseRef-{name}.toml"));
       if services.fs.exists(&path) {
           return Err(...);  // refuse to overwrite
       }
       let template = render_license_template(name);
       services.fs.write(&path, &template).change_context(AddLicenseError)?;
       Ok(path)
   }
   ```

   Template content (permissive defaults, every field commented):

   ```toml
   # License definition for LicenseRef-<name>.
   #
   # This file defines the obligation grid for a license. After editing, also
   # create LICENSES/LicenseRef-<name>.txt with the full license text — audit
   # will FAIL until that file exists.
   #
   # The `id` is "LicenseRef-<name>". `LicenseRef-` is the SPDX convention for
   # custom licenses not in the SPDX list. The .txt file must share this id.

   id = "LicenseRef-<name>"
   name = "TODO: human-readable license name"
   url = ""

   [terms]
   # Whether attribution (title + author + source) is required.
   requires_attribution = false
   # Whether the license notice must be reproduced with the asset.
   requires_license_notice = false
   # Whether source disclosure is required on distribution (e.g. GPL).
   requires_source_disclosure = false
   # Derivative works policy: "disallowed" (ND), "allowed" (permissive), or "share-alike" (SA/GPL).
   derivatives = "allowed"
   # Whether modified assets must carry a "(modified from original)" notice.
   requires_modification_notice = false
   # Whether commercial use is permitted.
   allows_commercial_use = true
   # Whether redistribution of the raw asset is permitted.
   allows_redistribution = true
   # Whether this license has clauses the grid can't verify; fails audit until
   # the id is listed in manual_review_acknowledged in auditah.toml.
   manual_review = false
   ```

2. **Add `cli/add_license_cmd.rs`:** `AddLicenseCmd` struct (positional `name`
   + `--root` default `.`), `run` that calls `write_license_template` and prints
   the path.

3. **Wire in `main.rs`:** Add `Command::AddLicense(AddLicenseCmd)` to the enum
   and dispatch. Import `add_license_cmd::AddLicenseCmd`.

4. **Scrap `init-licenses`:** Delete `src/init_licenses.rs` and
   `src/cli/init_licenses_cmd.rs`. Remove `Command::InitLicenses` arm + import +
   dispatch. Reword `audit.rs:135` message.

### Phase 3: Test migration

1. **Add `LicenseTerms::permissive()`** in `src/model/terms.rs` (the default
   baseline; also used by the `add-license` template and `LicenseSpec::new`).
2. **Term fixtures in `tests/common/mod.rs`:** Add named archetypes
   (`permissive_terms()`, `share_alike_terms()`, `non_commercial_terms()`) — or
   thin wrappers over `LicenseTerms::permissive()` with overrides. Remove
   `services()` and `seed_licenses()` shortcuts.
3. **Migrate every SPDX-id-dependent test** to build its registry via
   `LicenseRegistry::builder().license(LicenseSpec::new("LicenseRef-..."))` and
   reference `LicenseRef-*` ids in sidecars/records.

### Phase 4: Verification

Run build/clippy/fmt/test; verify each acceptance criterion.

---

## Anti-Goals (Out of Scope)

- **No SPDX-data fetching / network calls.** The well-known-license fetch
  mechanism is deferred to a future task.
- **No `--force` flag on `add-license`.** Refuse-to-overwrite is the default and
  only behavior; users edit or `rm` manually.
- **No interactive prompting in `add-license`.** It's a template generator, not
  a wizard.
- **No "embedded well-known license" story.** Everything collapses to
  `LicenseRef-*` until a future task designs the well-known mechanism.
- **No changes to the `Overrides` model, `effective_terms`, or the audit
  obligation checks** (derivatives, redistribution, manual_review) — those are
  settled from the prior task.
- **No `manual_review_acknowledged` config changes.** That config field stays as
  is.

---

## Edge Cases & Gotchas

1. **`discovery.rs` exclude pattern.** `src/discovery.rs:22` has
   `**/licenses/**` (lowercase). After consolidating to `LICENSES/`, this becomes
   `**/LICENSES/**`. Case-sensitive on Unix; getting this wrong means audit scans
   the license grid files as assets.
2. **Case sensitivity of `LICENSES/`.** On case-insensitive filesystems (macOS
   default), `licenses/` and `LICENSES/` collide. After consolidation, ensure all
   code uses the uppercase form consistently. The test `FakeFs` should be checked
   for case sensitivity behavior.
3. **`add-license` id derivation.** `<name>` is used verbatim to form
   `LicenseRef-<name>`. No sanitization in this task — a name with spaces or
   special chars produces a weird id. Accepted for now (the user edits the
   template); could validate in a follow-up.
4. **`deny_unknown_fields` on `LicenseRegistryEntry`.** After dropping the `text`
   field, a project-local TOML that still has `text = "..."` must be *rejected*
   (not silently ignored). Verify `#[serde(deny_unknown_fields)]` is present on
   the struct.
5. **Builder's `commit` serializes via `toml::to_string`.** The
   `LicenseRegistryEntry` must derive `Serialize`. It already does
   (`src/model/license.rs:11`). Ensure `notes: Option` uses
   `skip_serializing_if = "Option::is_none"` (it does).
6. **`Services::real` when no `LICENSES/` exists.** `LicenseRegistry::load`
   handles a missing dir gracefully (returns empty registry via the
   `!fs.exists` early return in `merge_project_local`). No change needed.
7. **The `cc_by_requires_attribution` / `cc0_does_not_require_attribution`
   tests** (registry.rs:142, 152) reference embedded SPDX ids. These become
   builder-constructed `LicenseRef-*` entries with explicit terms — the test
   intent (a license can require attribution / not) is preserved, just no longer
   coupled to specific SPDX ids.
8. **`clippy::unwrap_used`/`expect_used` lints** (enabled in the prior task).
   New production code in `add_license.rs` must use `?`/`Result`, not
   `unwrap`/`expect`. Test code gets `#[allow(...)]` per the established pattern.

---

## Navigation Anchors

- **`LicenseRegistry::load`** (`src/registry.rs:302`) — the production entry
  point; rewrite to start from empty map + read `LICENSES/`.
- **`merge_project_local`** (`src/registry.rs:343`) — the directory read; change
  `licenses` → `LICENSES`.
- **`read_and_parse_local`** (`src/registry.rs:372`) — the inline-text guard to
  delete.
- **`LicenseRegistryEntry`** (`src/model/license.rs:12`) — drop the `text` field.
- **`Command` enum** (`src/main.rs:25`) — remove `InitLicenses`, add
  `AddLicense`.
- **`audit.rs:135`** — the `MissingLicenseText` message to reword.
- **`Services::real`** (`src/services.rs:38`) — unchanged path but the dir it
  reads is now `LICENSES/`.
- **`discovery.rs:22`** — the exclude glob to update.

---

## Dependency Mappings

- **No new external crates.** `clap`, `toml`, `error_stack`, `wherror`,
  `rstest`, `walkdir`, `globset` are all already in `Cargo.toml`.
- **Internal:** `add_license_cmd` depends on a new `add_license` module and
  `Services`. The `LicenseRegistryBuilder` depends on `LicenseRegistryEntry` +
  `LicenseTerms`. `LicenseTerms::permissive()` is a new associated fn on the
  existing `LicenseTerms` type.

---

## Test Strategies

### Phase 1 (registry + consolidation)
- **Update** `embedded_registry_contains_all_four_expected_ids` → DELETE (no
  embedded licenses). Replace with a test asserting `LicenseRegistry::empty()`
  has 0 entries.
- **Update** `registry_lookup_returns_entry_for_known_id` (rstest over 4 SPDX
  ids) → rewrite to a builder-constructed `LicenseRef-*` lookup.
- **Update** `registry_lookup_returns_none_for_unknown_id` → keep, but use
  builder.
- **Update** `cc_by_requires_attribution` / `cc0_does_not_require_attribution`
  → rewrite as builder entries with explicit terms.
- **Update** `effective_terms_applies_overrides` (rstest) → builder base.
- **Update** `project_local_license_overrides_embedded_by_id` → becomes
  "project-local override by id" (no embedded base); the `text = "override"`
  line in the fixture must be removed (field dropped).
- **Update** `entry_round_trips_through_toml` / `embedded_license_entry_round_trips_through_toml`
  → drop `text` from the fixture; the embedded round-trip test is deleted (no
  embedded).
- **Add** `registry_builder_build_is_in_memory` — `.build()` resolves added
  specs; no disk touched.
- **Add** `registry_builder_commit_writes_and_loads` — `.commit(root)` writes
  `LICENSES/<id>.toml` and loads back.
- **Add** `merge_reads_from_uppercase_LICENSES_dir` — regression guard for the
  directory consolidation.
- **Add** `inline_text_field_rejected_by_deny_unknown_fields` — a project-local
  TOML with `text = "..."` fails to parse.

### Phase 2 (`add-license` + scrap)
- **Add** `add_license_writes_permissive_template` (case 1) — asserts the file
  content + id derivation.
- **Add** `template_has_comment_on_every_field` (case 2) — every `[terms]` key
  preceded by `#`.
- **Add** `template_header_explains_id_relationship` (case 3) — header mentions
  `LicenseRef-` and `LICENSES/<id>.txt`.
- **Add** `add_license_refuses_to_overwrite` (case 4).
- **Add** `add_license_respects_root_flag` (case 5).
- **Delete** all `init-licenses` tests in `src/init_licenses.rs`.
- **Update** `MissingLicenseText` message test in `src/audit/report.rs` to drop
  the `init-licenses` substring.

### Phase 3 (migration)
- For each of the 7 integration test files + unit tests: replace
  `services()`/`seed_licenses()` with a builder-constructed registry; replace
  SPDX id literals with `LicenseRef-*`. Each test's *intent* is preserved (it
  tests the same behavior), just no longer coupled to specific SPDX ids.

---

## Acceptance Criteria

1. `LICENSES/` is the sole license directory — both `.toml` and `.txt`; the
   lowercase `licenses/` path is gone.
2. `auditah add-license Foo` writes `./LICENSES/LicenseRef-Foo.toml` with
   permissive defaults and a `#` comment on every field.
3. The template header explains the `id` ↔ `LICENSES/<id>.txt` relationship.
4. Re-running `add-license Foo` errors (no `--force`); `--root` writes to the
   given project root.
5. `auditah init-licenses` no longer exists; `LicenseRegistry` has no embedded
   entries.
6. `LicenseRegistryEntry` has no `text` field; the inline-text guard is gone.
7. `LicenseRegistry::builder()` lets tests construct a registry in-memory;
   `.commit(root)` writes `LICENSES/*.toml` + loads for disk tests.
8. A `LicenseRef-*` license resolves iff its `LICENSES/<id>.toml` exists;
   `audit` gates on `LICENSES/<id>.txt` presence (unchanged).
9. `cargo build --tests`, `cargo clippy --tests` (with `unwrap_used`/
   `expect_used`), `cargo fmt --check`, and the full suite are clean.
10. No test references a "well-known" SPDX id as resolvable; all use
    `LicenseRef-*` via the builder.

---

## Phases

### Phase 1: Production — registry + directory consolidation
Drop `embedded_entries()`/`embedded_only()`, the `text` field, `include_str!`,
and the inline-text guard. Rewrite `LicenseRegistry::load` to start from an
empty map and read `LICENSES/*.toml`. Update `discovery.rs` exclude glob. Add
`LicenseRegistryBuilder` + `LicenseSpec` + `LicenseRegistry::builder()`/
`empty()`. Add `LicenseTerms::permissive()`.

### Phase 2: Production — `add-license` + scrap `init-licenses`
Add `src/add_license.rs` (`render_license_template` +
`write_license_template`) and `src/cli/add_license_cmd.rs` (`AddLicenseCmd` +
`run`); wire `Command::AddLicense` in `main.rs`. Delete `src/init_licenses.rs`
and `src/cli/init_licenses_cmd.rs`; remove the `Command::InitLicenses`
arm/import/dispatch; reword the `audit.rs:135` message.

### Phase 3: Tests — builder + migration
Add term fixtures in `tests/common/mod.rs`; remove `services()`/
`seed_licenses()`. Migrate every SPDX-id-dependent test (7 integration files +
unit tests in `registry.rs`/`add.rs`/`resolver.rs`/etc.) to build its own
registry via the builder and reference `LicenseRef-*`.

### Phase 4: Verification
Build/clippy/fmt/test clean; each acceptance criterion verified.

---

## Test Cases

| # | Case | Expected |
|---|---|---|
| 1 | `add-license Foo` | writes `LICENSES/LicenseRef-Foo.toml`; `id = "LicenseRef-Foo"`; permissive defaults |
| 2 | template field coverage | every `[terms]` field has a `#` comment explaining it |
| 3 | template header | explains `id`/`LicenseRef-`/`LICENSES/<id>.txt` |
| 4 | `add-license Foo` when file exists | errors, does not overwrite |
| 5 | `add-license Foo --root /tmp/p` | writes to `/tmp/p/LICENSES/LicenseRef-Foo.toml` |
| 6 | registry loads `LicenseRef-Foo` from `LICENSES/*.toml` | resolves; empty otherwise |
| 7 | no `LICENSES/*.toml` present | registry empty; audit FAILs assets as `UnknownLicense` |
| 8 | `LicenseRef-Foo` + `LICENSES/LicenseRef-Foo.txt` present | audit text-check clean |
| 9 | registry builder in-memory | `.build()` yields a registry resolving the added specs; no disk touched |
| 10 | registry builder `.commit(root)` | writes `LICENSES/<id>.toml` then loads back; round-trips |
| 11 | `init-licenses` removed | no command variant; build clean |
| 12 | `MissingLicenseText` message | no longer mentions `init-licenses`; points at `LICENSES/<id>.txt` |
| 13 | inline `text` field in TOML | rejected by `deny_unknown_fields` (field removed from schema) |
| 14 | migrated tests | no test references a well-known SPDX id as resolvable; all use `LicenseRef-*` via the builder |
