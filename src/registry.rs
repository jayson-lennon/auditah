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
