//! Configuration resolution: for each asset, find its attribution config by
//! precedence — adjacent sidecar > nearest ancestor manifest > uncovered.

use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::model::attribution::AttributionRecord;
use crate::services::FsService;

/// Error resolving an asset's config.
#[derive(Debug, Error)]
#[error(debug)]
pub struct ResolveError;

/// Sidecar suffix appended to an asset's filename.
pub const SIDECAR_SUFFIX: &str = ".attr.toml";

/// Directory manifest filename.
pub const MANIFEST_FILENAME: &str = "manifest.toml";

/// Where a resolved config came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionSource {
    /// Resolved from an adjacent `<asset>.attr.toml` sidecar.
    Sidecar(PathBuf),
    /// Resolved from the nearest ancestor `manifest.toml`.
    Manifest(PathBuf),
    /// No config found — the asset is uncovered.
    None,
}

/// A fully resolved asset: its path, the attribution record that applies, and
/// where that record came from.
#[derive(Debug, Clone)]
pub struct ResolvedAsset {
    /// Absolute path to the asset file.
    pub asset_path: PathBuf,
    /// The attribution record that applies (default if uncovered).
    pub record: Option<AttributionRecord>,
    /// Where the record was resolved from.
    pub source: ResolutionSource,
}

/// Build the sidecar path for a given asset: `<asset>.attr.toml`.
#[must_use]
pub fn sidecar_path(asset: &Path) -> PathBuf {
    let mut s = asset.as_os_str().to_os_string();
    s.push(SIDECAR_SUFFIX);
    PathBuf::from(s)
}

/// Walk up from the asset's directory; return the path of the first
/// `manifest.toml` found, or `None` if none exists up to (and including) `root`.
fn find_nearest_manifest(fs: &FsService, asset: &Path, root: &Path) -> Option<PathBuf> {
    let start = asset.parent()?;
    let root_parent = root.parent().unwrap_or(root);
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(MANIFEST_FILENAME);
        if fs.exists(&candidate) {
            return Some(candidate);
        }
        if d == root || d == root_parent {
            break;
        }
        dir = d.parent();
    }
    None
}

/// Resolve a single asset's config. Precedence:
/// 1. Adjacent `<asset>.attr.toml` sidecar.
/// 2. Nearest ancestor `manifest.toml`.
/// 3. Uncovered (`None`).
///
/// # Errors
///
/// Returns an error if a sidecar or manifest exists but cannot be read or parsed.
pub fn resolve(fs: &FsService, asset: &Path, root: &Path) -> Result<ResolvedAsset, Report<ResolveError>> {
    // 1. Sidecar.
    let sidecar = sidecar_path(asset);
    if fs.exists(&sidecar) {
        let record = read_attribution(fs, &sidecar)?;
        return Ok(ResolvedAsset {
            asset_path: asset.to_path_buf(),
            record: Some(record),
            source: ResolutionSource::Sidecar(sidecar),
        });
    }
    // 2. Nearest manifest.
    if let Some(manifest) = find_nearest_manifest(fs, asset, root) {
        let record = read_attribution(fs, &manifest)?;
        return Ok(ResolvedAsset {
            asset_path: asset.to_path_buf(),
            record: Some(record),
            source: ResolutionSource::Manifest(manifest),
        });
    }
    // 3. Uncovered.
    Ok(ResolvedAsset {
        asset_path: asset.to_path_buf(),
        record: None,
        source: ResolutionSource::None,
    })
}

/// Read and parse an attribution file (sidecar or manifest) into an
/// [`AttributionRecord`]. Uses `toml::from_str` — comment preservation matters
/// only on *write* (add/init-pack commands use `toml_edit`); reading discards
/// comments harmlessly.
///
/// # Errors
///
/// Returns an error if the file cannot be read or is not valid TOML matching
/// the attribution schema.
pub fn read_attribution(
    fs: &FsService,
    path: &Path,
) -> Result<AttributionRecord, Report<ResolveError>> {
    let content = fs
        .read_to_string(path)
        .change_context(ResolveError)
        .attach("failed to read attribution file")?;
    toml::from_str(&content)
        .change_context(ResolveError)
        .attach("failed to parse attribution TOML")
        .attach(path.display().to_string())
}

/// Detect orphan sidecars: a sidecar `<asset>.attr.toml` whose `<asset>` file
/// does not exist. Returns the paths of orphaned sidecars.
///
/// # Errors
///
/// Returns an error if the filesystem walk fails.
pub fn find_orphan_sidecars(fs: &FsService, all_files: &[PathBuf]) -> Vec<PathBuf> {
    all_files
        .iter()
        .filter(|p| {
            let s = p.to_string_lossy();
            s.ends_with(SIDECAR_SUFFIX)
        })
        .filter(|sidecar| {
            // Strip the suffix to recover the asset path; check existence via service.
            let asset = strip_sidecar_suffix(sidecar);
            !fs.exists(&asset)
        })
        .cloned()
        .collect()
}

/// Remove the `.attr.toml` suffix from a sidecar path, returning the asset path.
fn strip_sidecar_suffix(sidecar: &Path) -> PathBuf {
    let s = sidecar.to_string_lossy();
    let stripped = s.strip_suffix(SIDECAR_SUFFIX).unwrap_or(&s);
    PathBuf::from(stripped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::fs::{FsBackend, FsError};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeFs {
        files: Mutex<HashMap<PathBuf, String>>,
    }
    impl FsBackend for FakeFs {
        fn read_to_string(&self, p: &Path) -> Result<String, Report<FsError>> {
            self.files
                .lock()
                .unwrap()
                .get(p)
                .cloned()
                .ok_or_else(|| Report::new(FsError))
        }
        fn write(&self, _p: &Path, _c: &str) -> Result<(), Report<FsError>> {
            Ok(())
        }
        fn list_dir(&self, _p: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
            Ok(Vec::new())
        }
        fn walk(&self, _root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
            Ok(self.files.lock().unwrap().keys().cloned().collect())
        }
        fn exists(&self, p: &Path) -> bool {
            self.files.lock().unwrap().contains_key(p) || p.exists()
        }
        fn name(&self) -> &'static str {
            "FakeFs"
        }
    }

    fn fake_record_toml(license: &str) -> String {
        format!(
            r#"title = "T"
author = "A"
year = 2020
license = "{license}"
source = "https://example.com"
"#
        )
    }

    fn fs_with(files: &[(&str, &str)]) -> FsService {
        let mut map = HashMap::new();
        for (k, v) in files {
            map.insert(PathBuf::from(k), (*v).to_string());
        }
        FsService::new(std::sync::Arc::new(FakeFs {
            files: Mutex::new(map),
        }))
    }

    #[test]
    fn sidecar_path_appends_suffix() {
        let p = sidecar_path(Path::new("/proj/sword.glb"));
        assert_eq!(p, PathBuf::from("/proj/sword.glb.attr.toml"));
    }

    #[test]
    fn sidecar_path_preserves_spaces_in_name() {
        let p = sidecar_path(Path::new("/proj/Gunny Sack.glb"));
        assert_eq!(p, PathBuf::from("/proj/Gunny Sack.glb.attr.toml"));
    }

    #[test]
    fn resolve_returns_uncovered_when_no_config() {
        let fs = fs_with(&[("/proj/sword.glb", "")]);
        let r = resolve(&fs, Path::new("/proj/sword.glb"), Path::new("/proj")).unwrap();
        assert_eq!(r.source, ResolutionSource::None);
        assert!(r.record.is_none());
    }

    #[test]
    fn resolve_uses_sidecar_when_present() {
        let fs = fs_with(&[
            ("/proj/sword.glb", ""),
            ("/proj/sword.glb.attr.toml", &fake_record_toml("CC-BY-3.0")),
        ]);
        let r = resolve(&fs, Path::new("/proj/sword.glb"), Path::new("/proj")).unwrap();
        assert!(matches!(r.source, ResolutionSource::Sidecar(_)));
        assert_eq!(r.record.unwrap().license, "CC-BY-3.0");
    }

    #[test]
    fn resolve_uses_nearest_manifest_when_no_sidecar() {
        let fs = fs_with(&[
            ("/proj/assets/sword.glb", ""),
            ("/proj/assets/manifest.toml", &fake_record_toml("CC0-1.0")),
        ]);
        let r = resolve(
            &fs,
            Path::new("/proj/assets/sword.glb"),
            Path::new("/proj"),
        )
        .unwrap();
        assert!(matches!(r.source, ResolutionSource::Manifest(_)));
        assert_eq!(r.record.unwrap().license, "CC0-1.0");
    }

    #[test]
    fn subdir_manifest_overrides_parent_manifest() {
        let parent = fake_record_toml("CC0-1.0");
        let child = fake_record_toml("MIT");
        let fs = fs_with(&[
            ("/proj/manifest.toml", &parent),
            ("/proj/sub/manifest.toml", &child),
            ("/proj/sub/sword.glb", ""),
        ]);
        let r = resolve(&fs, Path::new("/proj/sub/sword.glb"), Path::new("/proj")).unwrap();
        assert!(matches!(r.source, ResolutionSource::Manifest(_)));
        assert_eq!(r.record.unwrap().license, "MIT");
    }

    #[test]
    fn sidecar_overrides_manifest_in_same_dir() {
        let fs = fs_with(&[
            ("/proj/sword.glb", ""),
            ("/proj/sword.glb.attr.toml", &fake_record_toml("MIT")),
            ("/proj/manifest.toml", &fake_record_toml("CC0-1.0")),
        ]);
        let r = resolve(&fs, Path::new("/proj/sword.glb"), Path::new("/proj")).unwrap();
        assert!(matches!(r.source, ResolutionSource::Sidecar(_)));
        assert_eq!(r.record.unwrap().license, "MIT");
    }

    #[test]
    fn orphan_sidecar_detected_when_asset_missing() {
        // real.glb exists; ghost.glb does not.
        let fs = fs_with(&[
            ("/proj/real.glb", ""),
            ("/proj/real.glb.attr.toml", ""),
            ("/proj/ghost.glb.attr.toml", ""),
        ]);
        let all = vec![
            PathBuf::from("/proj/ghost.glb.attr.toml"),
            PathBuf::from("/proj/real.glb"),
            PathBuf::from("/proj/real.glb.attr.toml"),
        ];
        let orphans = find_orphan_sidecars(&fs, &all);
        // ghost.glb.attr.toml has no ghost.glb → orphan.
        // real.glb.attr.toml has real.glb → not orphan.
        assert_eq!(orphans.len(), 1);
        assert!(orphans[0].to_string_lossy().contains("ghost"));
    }
}
