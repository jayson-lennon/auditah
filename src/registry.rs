//! License registry: project-local `LICENSES/*.toml` definitions loaded at
//! runtime. No embedded licenses — every license is `LicenseRef-*` authored
//! via `add-license` (or hand-placed in `LICENSES/`).
//!
//! Each license is two files in a single `LICENSES/` directory:
//! `<id>.toml` (metadata + terms grid) and `<id>.txt` (full legal text). The
//! `.toml` is parsed here; the `.txt` presence is checked at audit time.
//!
//! Tests construct a registry in-memory via [`LicenseRegistry::builder`], or via
//! [`LicenseRegistryBuilder::commit`] for tests that need files on disk.

use std::{collections::HashMap, path::Path, path::PathBuf};

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::model::license::LicenseRegistryEntry;
use crate::model::terms::LicenseTerms;
use crate::services::FsService;
use crate::well_known;

/// Error loading the license registry.
#[derive(Debug, Error)]
#[error(debug)]
pub struct RegistryError;

/// The license registry: `LICENSES/*.toml` definitions loaded at runtime.
#[derive(Debug, Clone)]
pub struct LicenseRegistry {
    entries: HashMap<String, LicenseRegistryEntry>,
}

impl LicenseRegistry {
    /// Load the registry from `LICENSES/*.toml` in `project_root`.
    ///
    /// Starts from an empty map — there are no embedded licenses. A missing
    /// `LICENSES/` directory yields an empty registry (no error).
    ///
    /// # Errors
    /// Returns `RegistryError` if a `LICENSES/*.toml` fails to parse or read.
    pub fn load(fs: &FsService, project_root: &Path) -> Result<Self, Report<RegistryError>> {
        let mut entries = HashMap::new();
        seed_well_known(&mut entries);
        merge_project_local(fs, project_root, &mut entries)?;
        Ok(Self { entries })
    }

    /// An empty registry. Used when no `LICENSES/` directory exists and by tests
    /// that want the trivial empty case.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Begin a fluent builder. Add licenses via `.license(spec)`, then `.build()`
    /// (in-memory) or `.commit(root, fs)` (writes `LICENSES/*.toml` + loads).
    #[must_use]
    pub fn builder() -> LicenseRegistryBuilder {
        LicenseRegistryBuilder::default()
    }

    /// Look up a license by id. `None` if unknown.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&LicenseRegistryEntry> {
        self.entries.get(id)
    }

    /// Iterate over all registry entries.
    pub fn entries(&self) -> impl Iterator<Item = &LicenseRegistryEntry> {
        self.entries.values()
    }

    /// Number of registered licenses.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Seed the registry with authored well-known SPDX grids parsed from the
/// embedded corpus (`well_known_licenses/*.toml` zipped into the binary).
/// Project-local `LICENSES/<id>.toml` files override these via the subsequent
/// `merge_project_local`.
fn seed_well_known(entries: &mut HashMap<String, LicenseRegistryEntry>) {
    for canonical in well_known::authored_grid_ids() {
        let Some(toml_str) = well_known::grid_for(&canonical) else {
            continue;
        };
        let Ok(entry): Result<LicenseRegistryEntry, _> = toml::from_str(&toml_str) else {
            continue;
        };
        entries.insert(entry.id.clone(), entry);
    }
}

/// Read each `<project_root>/LICENSES/*.toml`, parse it, and merge into `entries`
/// by `id`.
fn merge_project_local(
    fs: &FsService,
    project_root: &Path,
    entries: &mut HashMap<String, LicenseRegistryEntry>,
) -> Result<(), Report<RegistryError>> {
    let licenses_dir = project_root.join("LICENSES");
    if !fs.exists(&licenses_dir) {
        return Ok(());
    }
    let toml_paths = list_local_tomls(fs, &licenses_dir)?;
    for path in toml_paths {
        let entry = read_and_parse_local(fs, &path)?;
        entries.insert(entry.id.clone(), entry);
    }
    Ok(())
}

/// List `*.toml` files in the `LICENSES/` dir.
fn list_local_tomls(fs: &FsService, dir: &Path) -> Result<Vec<PathBuf>, Report<RegistryError>> {
    Ok(fs
        .list_dir(dir)
        .change_context(RegistryError)
        .attach("failed to list project-local licenses directory".to_string())?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
        .collect())
}

/// Read and parse one project-local license TOML.
fn read_and_parse_local(
    fs: &FsService,
    path: &Path,
) -> Result<LicenseRegistryEntry, Report<RegistryError>> {
    let content = fs
        .read_to_string(path)
        .change_context(RegistryError)
        .attach("failed to read project-local license file".to_string())
        .attach(path.display().to_string())?;
    let entry: LicenseRegistryEntry = toml::from_str(&content)
        .change_context(RegistryError)
        .attach("failed to parse project-local license TOML".to_string())
        .attach(path.display().to_string())?;
    Ok(entry)
}

/// Fluent builder for a [`LicenseRegistry`]. Used by tests to construct a
/// registry in-memory (`.build()`) or commit it to disk (`.commit(root, fs)`).
#[derive(Debug, Clone, Default)]
pub struct LicenseRegistryBuilder {
    specs: Vec<LicenseSpec>,
}

impl LicenseRegistryBuilder {
    /// Add a license spec. Chainable.
    #[must_use]
    pub fn license(mut self, spec: LicenseSpec) -> Self {
        self.specs.push(spec);
        self
    }

    /// Build the registry in-memory. No disk touched. The common case.
    #[must_use]
    pub fn build(self) -> LicenseRegistry {
        let mut entries = HashMap::new();
        for spec in self.specs {
            entries.insert(spec.id.clone(), spec.into_entry());
        }
        LicenseRegistry { entries }
    }

    /// Write `LICENSES/<id>.toml` for each spec, then load the merged registry.
    /// For tests that need files on disk (add-license output, load, audit text-check).
    ///
    /// # Errors
    /// Returns `RegistryError` if a TOML serialize or write fails.
    pub fn commit(
        self,
        root: &Path,
        fs: &FsService,
    ) -> Result<LicenseRegistry, Report<RegistryError>> {
        let dir = root.join("LICENSES");
        for spec in &self.specs {
            let path = dir.join(format!("{}.toml", spec.id));
            let toml = toml::to_string(&spec.entry())
                .change_context(RegistryError)
                .attach("failed to serialize license template".to_string())
                .attach(spec.id.clone())?;
            fs.write(&path, &toml)
                .change_context(RegistryError)
                .attach("failed to write LICENSES/<id>.toml".to_string())
                .attach(path.display().to_string())?;
        }
        LicenseRegistry::load(fs, root)
    }
}

/// Specification for one license entry in a builder. Defaults to permissive terms.
#[derive(Debug, Clone)]
pub struct LicenseSpec {
    id: String,
    entry: LicenseRegistryEntry,
}

impl LicenseSpec {
    /// Create a spec for `id` with permissive default terms.
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            entry: LicenseRegistryEntry {
                id: id.to_string(),
                name: id.to_string(),
                url: String::new(),
                terms: LicenseTerms::permissive(),
                notes: None,
            },
        }
    }

    /// Replace the terms.
    #[must_use]
    pub fn terms(mut self, terms: LicenseTerms) -> Self {
        self.entry.terms = terms;
        self
    }

    /// Set the human-readable name.
    #[must_use]
    pub fn name(mut self, name: &str) -> Self {
        self.entry.name = name.to_string();
        self
    }

    /// Consume into the entry. (Builder's `.build()` consumes the spec.)
    fn into_entry(self) -> LicenseRegistryEntry {
        self.entry
    }

    /// Borrow the entry. (Builder's `.commit()` serializes without consuming.)
    fn entry(&self) -> &LicenseRegistryEntry {
        &self.entry
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::terms::Derivatives;
    use crate::services::fs::{FsService, RealFs};
    use std::sync::Arc;

    fn fs() -> FsService {
        FsService::new(Arc::new(RealFs::new()))
    }

    fn tmp_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    // --- Registry construction ---

    #[test]
    fn empty_registry_has_no_entries() {
        // Given an empty registry.
        let reg = LicenseRegistry::empty();

        // When checking its size.
        // Then it has zero entries.
        assert_eq!(reg.len(), 0);
        assert!(reg.is_empty());
    }

    #[test]
    fn builder_with_no_specs_is_empty() {
        // Given a builder with no specs.
        // When building.
        let reg = LicenseRegistry::builder().build();

        // Then the registry is empty.
        assert!(reg.is_empty());
    }

    #[test]
    fn builder_resolves_built_license() {
        // Given a builder with one LicenseRef-Asset spec.
        // When building.
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-Asset"))
            .build();

        // Then the license resolves by id.
        assert!(reg.get("LicenseRef-Asset").is_some());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn builder_terms_override_takes_effect() {
        // Given a builder with a share-alike spec.
        let terms = LicenseTerms::permissive().with_derivatives(Derivatives::ShareAlike);

        // When building.
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-Gpl").terms(terms))
            .build();

        // Then the built entry carries the share-alike terms.
        let entry = reg.get("LicenseRef-Gpl").expect("entry");
        assert_eq!(entry.terms.derivatives, Derivatives::ShareAlike);
    }

    // --- Registry load from disk ---

    #[test]
    fn commit_writes_and_loads_licenseref_entry() {
        // Given a temp root.
        let tmp = tmp_root();

        // When committing a LicenseRef-Foo spec.
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-Foo"))
            .commit(tmp.path(), &fs())
            .expect("commit");

        // Then the entry is present in the in-memory registry.
        assert!(
            reg.get("LicenseRef-Foo").is_some(),
            "commit must write + load the LicenseRef-Foo entry"
        );
    }

    #[test]
    fn load_reads_entry_from_uppercase_licenses_dir() {
        // Given a temp root with a committed LicenseRef-Foo entry.
        let tmp = tmp_root();
        LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-Foo"))
            .commit(tmp.path(), &fs())
            .expect("commit");

        // When re-loading from the same root (simulating app startup).
        let reloaded = LicenseRegistry::load(&fs(), tmp.path()).expect("load");

        // Then the LicenseRef-Foo entry resolves from uppercase LICENSES/.
        assert!(
            reloaded.get("LicenseRef-Foo").is_some(),
            "registry must read from uppercase LICENSES/"
        );
        assert!(
            tmp.path()
                .join("LICENSES")
                .join("LicenseRef-Foo.toml")
                .exists(),
            "grid must be at uppercase LICENSES/"
        );
    }

    #[test]
    fn load_missing_licenses_dir_yields_only_well_known_grids() {
        // Given a temp root with no LICENSES/ dir.
        let tmp = tmp_root();

        // When loading.
        let reg = LicenseRegistry::load(&fs(), tmp.path()).expect("load");

        // Then the registry is non-empty (seeded with authored well-known grids),
        // and contains no LicenseRef- (those must come from the project).
        assert!(
            !reg.is_empty(),
            "load must seed authored well-known grids even with no LICENSES/ dir"
        );
        assert!(
            reg.entries().all(|e| !e.id.starts_with("LicenseRef-")),
            "no LicenseRef- entries without project-local LICENSES/"
        );
    }

    #[test]
    fn load_rejects_malformed_toml() {
        // Given a LICENSES/ dir with a malformed TOML file.
        let tmp = tmp_root();
        let dir = tmp.path().join("LICENSES");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("LicenseRef-Bad.toml"), "not = valid = toml").unwrap();

        // When loading.
        let result = LicenseRegistry::load(&fs(), tmp.path());

        // Then loading fails.
        assert!(result.is_err());
    }

    // --- deny_unknown_fields enforcement ---

    #[test]
    fn load_rejects_inline_text_field() {
        // Given a LICENSES/ dir with a TOML carrying the removed `text` field.
        let tmp = tmp_root();
        let dir = tmp.path().join("LICENSES");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("LicenseRef-Text.toml"),
            "id = \"LicenseRef-Text\"\nname = \"x\"\nurl = \"\"\n".to_string()
                + "[terms]\n"
                + "requires_attribution = false\n"
                + "requires_license_notice = false\n"
                + "requires_source_disclosure = false\n"
                + "derivatives = \"allowed\"\n"
                + "requires_modification_notice = false\n"
                + "allows_commercial_use = true\n"
                + "allows_redistribution = true\n"
                + "manual_review = false\n"
                + "text = \"should be rejected\"\n",
        )
        .unwrap();

        // When loading.
        let result = LicenseRegistry::load(&fs(), tmp.path());

        // Then loading fails — the dropped `text` field is rejected.
        assert!(result.is_err());
    }
    // --- well-known seeding (Phase 6) ---

    #[test]
    fn load_seeds_authored_well_known_grids_from_embedded_corpus() {
        // Given a temp root with no LICENSES/ dir.
        let tmp = tmp_root();

        // When loading.
        let reg = LicenseRegistry::load(&fs(), tmp.path()).expect("load");

        // Then authored well-known SPDX ids resolve (no project-local files).
        assert!(
            reg.get("MIT").is_some(),
            "MIT grid must be seeded from the embedded corpus"
        );
        assert!(
            reg.get("CC-BY-4.0").is_some(),
            "CC-BY-4.0 grid must be seeded"
        );
    }

    #[test]
    fn project_local_grid_overrides_seeded_well_known_grid() {
        // Given a project-local MIT.toml overriding the embedded grid.
        let tmp = tmp_root();
        let dir = tmp.path().join("LICENSES");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("MIT.toml"),
            "id = \"MIT\"\nname = \"overridden\"\nurl = \"https://override\"\n".to_string()
                + "[terms]\n"
                + "requires_attribution = false\n"
                + "requires_license_notice = false\n"
                + "requires_source_disclosure = false\n"
                + "derivatives = \"disallowed\"\n"
                + "requires_modification_notice = false\n"
                + "allows_commercial_use = false\n"
                + "allows_redistribution = false\n"
                + "manual_review = false\n",
        )
        .unwrap();

        // When loading.
        let reg = LicenseRegistry::load(&fs(), tmp.path()).expect("load");

        // Then the project-local MIT entry wins (name overridden, derivatives set by it).
        let entry = reg.get("MIT").expect("MIT resolves");
        assert_eq!(entry.name, "overridden");
        assert_eq!(entry.terms.derivatives, Derivatives::Disallowed);
    }
}
