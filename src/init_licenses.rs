//! `auditah init-licenses` — write full license text files to `LICENSES/`.
//!
//! On-disk license text is the source of truth (option A in the dialectic). The
//! binary carries embedded seeds for common SPDX licenses so `init-licenses` can
//! write them out on first run; project-local `licenses/*.toml` entries carry
//! their own `text`. After generation, the on-disk files are authoritative and
//! editable. `audit` FAILs any referenced license id whose `LICENSES/<id>.txt`
//! is absent.

use std::path::Path;

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::services::Services;

/// Error writing license text files.
#[derive(Debug, Error)]
#[error(debug)]
pub struct InitLicensesError;

/// Outcome of `init_licenses`: how many files were written vs. skipped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InitOutcome {
    /// Files newly written.
    pub written: usize,
    /// Files skipped because they already existed with matching content.
    pub skipped: usize,
}

/// Write `LICENSES/<id>.txt` for every registry entry that has license text.
///
/// Existing files whose content already matches are skipped (idempotent).
/// Existing files that differ are **not** overwritten — on-disk text is the
/// source of truth; a divergence is surfaced via the returned error so a human
/// can decide whether to keep the edited version or reseed.
///
/// # Errors
///
/// Returns `InitLicensesError` if a file cannot be read or written.
pub fn init_licenses(
    services: &Services,
    root: &Path,
) -> Result<InitOutcome, Report<InitLicensesError>> {
    let dir = root.join("LICENSES");
    let mut outcome = InitOutcome::default();

    for entry in services.registry.entries() {
        if entry.text.is_empty() {
            continue;
        }
        let path = dir.join(format!("{}.txt", entry.id));
        if try_write(services, &path, &entry.text, &mut outcome)?.is_none() {
            outcome.written += 1;
        }
    }
    Ok(outcome)
}

/// Write `path` with `content` unless it already holds matching content.
///
/// Returns `Some(())` when skipped (already matches), `None` when written.
/// Errors when an existing file's content diverges (human-edited) — on-disk is
/// authoritative, so we refuse to clobber silently.
fn try_write(
    services: &Services,
    path: &Path,
    content: &str,
    outcome: &mut InitOutcome,
) -> Result<Option<()>, Report<InitLicensesError>> {
    if services.fs.exists(path) {
        let existing = services
            .fs
            .read_to_string(path)
            .change_context(InitLicensesError)
            .attach("failed to read existing LICENSES file")
            .attach(path.display().to_string())?;
        if existing == content {
            outcome.skipped += 1;
            return Ok(Some(()));
        }
        return Err(Report::new(InitLicensesError)
            .attach("LICENSES file diverges from registry text (on-disk is authoritative; edit deliberately)")
            .attach(path.display().to_string()));
    }
    services
        .fs
        .write(path, content)
        .change_context(InitLicensesError)
        .attach("failed to write LICENSES file")
        .attach(path.display().to_string())?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::LicenseRegistry;
    use crate::services::fs::{FsBackend, FsError, FsService};
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// A minimal in-memory FsBackend backed by a HashMap.
    struct FakeFs {
        files: Mutex<HashMap<PathBuf, String>>,
    }

    impl FakeFs {
        fn empty() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
            }
        }
    }

    impl FsBackend for FakeFs {
        fn read_to_string(&self, p: &Path) -> Result<String, Report<FsError>> {
            self.files
                .lock()
                .get(p)
                .cloned()
                .ok_or_else(|| Report::new(FsError))
        }
        fn write(&self, p: &Path, c: &str) -> Result<(), Report<FsError>> {
            self.files.lock().insert(p.to_path_buf(), c.to_string());
            Ok(())
        }
        fn list_dir(&self, _p: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
            Ok(Vec::new())
        }
        fn walk(&self, _root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
            Ok(Vec::new())
        }
        fn exists(&self, p: &Path) -> bool {
            self.files.lock().contains_key(p)
        }
        fn name(&self) -> &'static str {
            "FakeFs"
        }
    }

    #[test]
    fn writes_all_embedded_licenses() {
        let fs = FsService::new(Arc::new(FakeFs::empty()));
        let registry = LicenseRegistry::embedded_only();
        let services = Services::from_parts(fs.clone(), registry);
        let outcome = init_licenses(&services, Path::new("/proj")).unwrap();
        // CC0, CC-BY-3.0, MIT, OFL-1.1 — all seeded with text.
        assert_eq!(outcome.written, 4);
        assert_eq!(outcome.skipped, 0);
        for id in ["CC0-1.0", "CC-BY-3.0", "MIT", "OFL-1.1"] {
            let p = PathBuf::from(format!("/proj/LICENSES/{id}.txt"));
            assert!(fs.exists(&p), "{id}.txt should exist");
        }
    }

    #[test]
    fn skips_files_with_matching_content() {
        let fs = FsService::new(Arc::new(FakeFs::empty()));
        // Pre-seed MIT with its embedded text.
        let reg = LicenseRegistry::embedded_only();
        let mit_text = reg.get("MIT").unwrap().text.clone();
        fs.write(&PathBuf::from("/proj/LICENSES/MIT.txt"), &mit_text)
            .unwrap();
        let registry = LicenseRegistry::embedded_only();
        let services = Services::from_parts(fs.clone(), registry);
        let outcome = init_licenses(&services, Path::new("/proj")).unwrap();
        assert_eq!(outcome.skipped, 1, "MIT already matches");
        assert_eq!(outcome.written, 3, "other three written");
    }

    #[test]
    fn errors_when_existing_file_diverges() {
        let fs = FsService::new(Arc::new(FakeFs::empty()));
        // Pre-seed MIT with divergent (human-edited) text.
        fs.write(
            &PathBuf::from("/proj/LICENSES/MIT.txt"),
            "human-edited version",
        )
        .unwrap();
        let registry = LicenseRegistry::embedded_only();
        let services = Services::from_parts(fs, registry);
        let result = init_licenses(&services, Path::new("/proj"));
        assert!(result.is_err(), "divergent file must error, not clobber");
    }
}
