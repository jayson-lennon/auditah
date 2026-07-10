//! Embedded license registry definitions. Bundled into the binary at compile
//! time via `include_str!`. Project-local `licenses/*.toml` files merge on top
//! of these (override by `id`, or add new `LicenseRef-*` ids).
//!
//! Each license is two files: `<id>.toml` (metadata + terms) and `<id>.txt`
//! (full legal text). The loader composes the two into a `LicenseRegistryEntry`.

use std::{collections::HashMap, path::PathBuf};

use crate::model::license::LicenseRegistryEntry;

/// Metadata + full text for one embedded license.
struct EmbeddedDef {
    id: &'static str,
    toml: &'static str,
    text: &'static str,
}

/// All bundled license definitions. Add a new bundled license by appending here
/// and dropping a `<id>.toml` + `<id>.txt` pair in this directory.
const EMBEDDED: &[EmbeddedDef] = &[
    EmbeddedDef {
        id: "CC0-1.0",
        toml: include_str!("embedded_licenses/CC0-1.0.toml"),
        text: include_str!("embedded_licenses/CC0-1.0.txt"),
    },
    EmbeddedDef {
        id: "CC-BY-3.0",
        toml: include_str!("embedded_licenses/CC-BY-3.0.toml"),
        text: include_str!("embedded_licenses/CC-BY-3.0.txt"),
    },
    EmbeddedDef {
        id: "MIT",
        toml: include_str!("embedded_licenses/MIT.toml"),
        text: include_str!("embedded_licenses/MIT.txt"),
    },
    EmbeddedDef {
        id: "OFL-1.1",
        toml: include_str!("embedded_licenses/OFL-1.1.toml"),
        text: include_str!("embedded_licenses/OFL-1.1.txt"),
    },
];

/// Parse all embedded license definitions into a map keyed by `id`.
///
/// Panics at startup if any embedded TOML fails to parse or has a mismatched
/// `id` — these are compile-time-authored data, so malformed = bug.
#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn embedded_entries() -> HashMap<String, LicenseRegistryEntry> {
    let mut map = HashMap::new();
    for def in EMBEDDED {
        let mut entry: LicenseRegistryEntry = toml::from_str(def.toml)
            .unwrap_or_else(|e| panic!("embedded {id} TOML malformed: {e}", id = def.id));
        assert_eq!(
            entry.id,
            def.id,
            "embedded license id mismatch: TOML says {toml_id}, file is {file_id}",
            toml_id = entry.id,
            file_id = def.id
        );
        // Fill the full legal text from the companion .txt (TOML omits it).
        if entry.text.is_empty() {
            entry.text = def.text.to_string();
        }
        map.insert(entry.id.clone(), entry);
    }
    map
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::model::terms::{effective_terms, Derivatives, Overrides};

    #[test]
    fn embedded_registry_contains_all_four_expected_ids() {
        // Given the embedded license registry.
        let map = embedded_entries();

        // When checking the registry size and ids.
        // Then all four ids are present.
        assert_eq!(map.len(), 4, "expected CC0-1.0, CC-BY-3.0, MIT, OFL-1.1");
        for id in ["CC0-1.0", "CC-BY-3.0", "MIT", "OFL-1.1"] {
            assert!(map.contains_key(id), "missing embedded license {id}");
        }
    }

    #[test]
    fn embedded_mit_text_is_nonempty_and_canonical() {
        // Given the embedded MIT license entry.
        let map = embedded_entries();
        let mit = &map["MIT"];

        // When inspecting its text.
        // Then the text is populated and contains the canonical permission phrase.
        assert!(!mit.text.is_empty(), "MIT text should be filled from .txt");
        assert!(mit.text.contains("Permission is hereby granted"));
    }

    // --- Registry lookup + unknown-id rejection (rstest-parameterized) ---

    /// Parameterized over every embedded license id: each must be lookable-up.
    #[rstest::rstest]
    #[case::cc0("CC0-1.0")]
    #[case::cc_by_3("CC-BY-3.0")]
    #[case::mit("MIT")]
    #[case::ofl("OFL-1.1")]
    fn registry_lookup_returns_entry_for_known_id(#[case] id: &str) {
        // Given the embedded registry.
        let reg = LicenseRegistry::embedded_only();

        // When looking up a known id.
        let entry = reg
            .get(id)
            .unwrap_or_else(|| panic!("{id} should be in registry"));

        // Then the entry's id matches.
        assert_eq!(entry.id, id);
    }

    #[rstest::rstest]
    #[case::typo("CC0")]
    #[case::missing("GPL-3.0")]
    #[case::empty("")]
    #[case::ref_not_embedded("LicenseRef-Custom")]
    fn registry_lookup_returns_none_for_unknown_id(#[case] id: &str) {
        // Given the embedded registry.
        let reg = LicenseRegistry::embedded_only();

        // When looking up an unknown id.
        // Then lookup returns None.
        assert!(reg.get(id).is_none(), "{id:?} should NOT resolve");
    }

    // --- split: per-license attribution requirement ---

    #[test]
    fn cc_by_requires_attribution() {
        // Given the embedded registry.
        let reg = LicenseRegistry::embedded_only();

        // When inspecting CC-BY-3.0 terms.
        // Then attribution is required.
        assert!(reg.get("CC-BY-3.0").unwrap().terms.requires_attribution);
    }

    #[test]
    fn cc0_does_not_require_attribution() {
        // Given the embedded registry.
        let reg = LicenseRegistry::embedded_only();

        // When inspecting CC0-1.0 terms.
        // Then attribution is not required.
        assert!(!reg.get("CC0-1.0").unwrap().terms.requires_attribution);
    }

    // --- effective_terms override application (rstest-parameterized) ---

    #[rstest::rstest]
    #[case::no_override("CC-BY-3.0", Overrides::default(), true, true)]
    #[case::flip_commercial("CC-BY-3.0", Overrides { allows_commercial_use: Some(false), ..Default::default() }, true, false)]
    #[case::flip_attribution("CC-BY-3.0", Overrides { requires_attribution: Some(false), ..Default::default() }, false, true)]
    fn effective_terms_applies_overrides(
        #[case] license_id: &str,
        #[case] overrides: Overrides,
        #[case] expect_attr: bool,
        #[case] expect_comm: bool,
    ) {
        // Given the registry entry for `license_id`.
        let reg = LicenseRegistry::embedded_only();
        let base = &reg.get(license_id).unwrap().terms;

        // When applying the overrides.
        let eff = effective_terms(base, &overrides);

        // Then the effective terms reflect the override (or lack thereof).
        assert_eq!(eff.requires_attribution, expect_attr);
        assert_eq!(eff.allows_commercial_use, expect_comm);
    }

    // --- Project-local merge via FakeFs (no real filesystem) ---

    #[test]
    fn project_local_license_overrides_embedded_by_id() {
        // Given a project-local MIT.toml that overrides name + commercial use.
        use crate::services::fs::FsService;
        use crate::test_support::FakeFs;
        use std::path::Path;
        let toml = r#"
            id = "MIT"
            name = "MIT (overridden)"
            url = "https://example.com/mit"
            text = "override"
            [terms]
            requires_attribution = false
            requires_license_notice = true
            requires_source_disclosure = false
            derivatives = "allowed"
            requires_modification_notice = false
            allows_commercial_use = false
            allows_redistribution = true
            manual_review = false
        "#;
        let fs = FsService::new(Arc::new(FakeFs::with_files([(
            "/proj/licenses/MIT.toml",
            toml,
        )])));

        // When loading the merged registry.
        let reg = LicenseRegistry::load(&fs, Path::new("/proj")).unwrap();

        // Then the project-local entry overrides the embedded one by id.
        let mit = reg.get("MIT").unwrap();
        assert_eq!(mit.name, "MIT (overridden)");
        assert!(!mit.terms.allows_commercial_use);
    }

    #[test]
    fn project_local_licenseref_added_to_registry() {
        // Given a project-local LicenseRef entry not in the embedded set.
        use crate::services::fs::FsService;
        use crate::test_support::FakeFs;
        use std::path::Path;
        let toml = r#"
            id = "LicenseRef-StudioEULA"
            name = "Studio Custom EULA"
            url = "https://example.com/eula"
            text = "custom"
            [terms]
            requires_attribution = true
            requires_license_notice = true
            requires_source_disclosure = false
            derivatives = "disallowed"
            requires_modification_notice = false
            allows_commercial_use = true
            allows_redistribution = false
            manual_review = false
        "#;
        let fs = FsService::new(Arc::new(FakeFs::with_files([(
            "/proj/licenses/LicenseRef-StudioEULA.toml",
            toml,
        )])));

        // When loading the merged registry.
        let reg = LicenseRegistry::load(&fs, Path::new("/proj")).unwrap();

        // Then the LicenseRef is added (4 embedded + 1) and its terms are honored.
        assert_eq!(reg.len(), 5, "4 embedded + 1 LicenseRef");
        let custom = reg.get("LicenseRef-StudioEULA").unwrap();
        assert_eq!(custom.terms.derivatives, Derivatives::Disallowed);
    }
}

use std::path::Path;

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::services::FsService;

/// Error loading the license registry.
#[derive(Debug, Error)]
#[error(debug)]
pub struct RegistryError;

/// The merged license registry: embedded licenses + project-local overrides/additions.
#[derive(Debug, Clone)]
pub struct LicenseRegistry {
    entries: HashMap<String, LicenseRegistryEntry>,
}

impl LicenseRegistry {
    /// Load the registry: embedded licenses first, then project-local `licenses/*.toml`
    /// merged by `id` (project wins).
    ///
    /// # Errors
    /// Returns `RegistryError` if a project-local TOML fails to parse or read.
    pub fn load(fs: &FsService, project_root: &Path) -> Result<Self, Report<RegistryError>> {
        let mut entries = embedded_entries();
        merge_project_local(fs, project_root, &mut entries)?;
        Ok(Self { entries })
    }

    /// Build a registry from embedded licenses only (no project-local files).
    /// Used in tests that don't need a filesystem.
    #[must_use]
    pub fn embedded_only() -> Self {
        Self {
            entries: embedded_entries(),
        }
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

    /// Number of registered licenses (embedded + project-local).
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

/// Read each `<project_root>/licenses/*.toml`, parse it, and merge into `entries`
/// by `id`. Project-local entries override embedded ones with the same id.
fn merge_project_local(
    fs: &FsService,
    project_root: &Path,
    entries: &mut HashMap<String, LicenseRegistryEntry>,
) -> Result<(), Report<RegistryError>> {
    let local_dir = project_root.join("licenses");
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

/// List `*.toml` files in the local `licenses/` dir.
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
    // Custom LicenseRef-* entries must carry their own full license text;
    // there is no embedded fallback for them.
    if entry.id.starts_with("LicenseRef-") && entry.text.trim().is_empty() {
        return Err(Report::from(RegistryError)
            .attach("custom LicenseRef-* license has empty `text`".to_string())
            .attach(entry.id.clone())
            .attach(path.display().to_string()));
    }
    Ok(entry)
}
