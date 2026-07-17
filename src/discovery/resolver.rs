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
pub const MANIFEST_FILENAME: &str = "_manifest.toml";

/// Exclude glob matching the manifest at any depth. Kept adjacent to
/// `MANIFEST_FILENAME` so a rename stays a one-place edit; the
/// `manifest_exclude_glob_matches_filename` test guards against drift.
pub const MANIFEST_EXCLUDE_GLOB: &str = "**/_manifest.toml";

/// Where a resolved config came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionSource {
    /// Resolved from an adjacent `<asset>.attr.toml` sidecar.
    Sidecar(PathBuf),
    /// Resolved from the nearest ancestor `_manifest.toml`.
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
/// `_manifest.toml` found, or `None` if none exists up to (and including) `root`.
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
/// 2. Nearest ancestor `_manifest.toml`.
/// 3. Uncovered (`None`).
///
/// # Errors
///
/// Returns an error if a sidecar or manifest exists but cannot be read or parsed.
pub fn resolve(
    fs: &FsService,
    asset: &Path,
    root: &Path,
) -> Result<ResolvedAsset, Report<ResolveError>> {
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

/// Remove the `.attr.toml` suffix from a sidecar path, returning the asset path.
pub(crate) fn strip_sidecar_suffix(sidecar: &Path) -> PathBuf {
    let s = sidecar.to_string_lossy();
    let stripped = s.strip_suffix(SIDECAR_SUFFIX).unwrap_or(&s);
    PathBuf::from(stripped)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeFs;
    use std::sync::Arc;

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
        FsService::new(Arc::new(FakeFs::with_files(
            files.iter().map(|(p, c)| (*p, *c)),
        )))
    }

    #[test]
    fn sidecar_path_appends_suffix() {
        // Given an asset path.
        // When computing the sidecar path.
        let p = sidecar_path(Path::new("/proj/sword.glb"));

        // Then the .attr.toml suffix is appended.
        assert_eq!(p, PathBuf::from("/proj/sword.glb.attr.toml"));
    }

    #[test]
    fn sidecar_path_preserves_spaces_in_name() {
        // Given an asset path containing spaces.
        // When computing the sidecar path.
        let p = sidecar_path(Path::new("/proj/Gunny Sack.glb"));

        // Then spaces are preserved in the sidecar path.
        assert_eq!(p, PathBuf::from("/proj/Gunny Sack.glb.attr.toml"));
    }

    #[test]
    fn resolve_returns_uncovered_when_no_config() {
        // Given an asset with no sidecar and no manifest.
        let fs = fs_with(&[("/proj/sword.glb", "")]);

        // When resolving.
        let r = resolve(&fs, Path::new("/proj/sword.glb"), Path::new("/proj")).unwrap();

        // Then the source is None and no record is present.
        assert_eq!(r.source, ResolutionSource::None);
        assert!(r.record.is_none());
    }

    #[test]
    fn resolve_uses_sidecar_when_present() {
        // Given an asset with a sidecar.
        let fs = fs_with(&[
            ("/proj/sword.glb", ""),
            (
                "/proj/sword.glb.attr.toml",
                &fake_record_toml("LicenseRef-CcBy"),
            ),
        ]);

        // When resolving.
        let r = resolve(&fs, Path::new("/proj/sword.glb"), Path::new("/proj")).unwrap();

        // Then the sidecar is used and its license is parsed.
        assert!(matches!(r.source, ResolutionSource::Sidecar(_)));
        assert_eq!(r.record.unwrap().license, "LicenseRef-CcBy");
    }

    #[test]
    fn resolve_uses_nearest_manifest_when_no_sidecar() {
        // Given an asset with no sidecar but a directory manifest.
        let fs = fs_with(&[
            ("/proj/assets/sword.glb", ""),
            (
                "/proj/assets/_manifest.toml",
                &fake_record_toml("LicenseRef-Cc0"),
            ),
        ]);

        // When resolving.
        let r = resolve(&fs, Path::new("/proj/assets/sword.glb"), Path::new("/proj")).unwrap();

        // Then the manifest is used and its license is parsed.
        assert!(matches!(r.source, ResolutionSource::Manifest(_)));
        assert_eq!(r.record.unwrap().license, "LicenseRef-Cc0");
    }

    #[test]
    fn subdir_manifest_overrides_parent_manifest() {
        // Given a parent and subdir manifest with different licenses.
        let parent = fake_record_toml("LicenseRef-Cc0");
        let child = fake_record_toml("LicenseRef-Mit");
        let fs = fs_with(&[
            ("/proj/_manifest.toml", &parent),
            ("/proj/sub/_manifest.toml", &child),
            ("/proj/sub/sword.glb", ""),
        ]);

        // When resolving the subdir asset.
        let r = resolve(&fs, Path::new("/proj/sub/sword.glb"), Path::new("/proj")).unwrap();

        // Then the subdir manifest wins (nearest).
        assert!(matches!(r.source, ResolutionSource::Manifest(_)));
        assert_eq!(r.record.unwrap().license, "LicenseRef-Mit");
    }

    #[test]
    fn sidecar_overrides_manifest_in_same_dir() {
        // Given a dir with both a manifest and a per-file sidecar.
        let fs = fs_with(&[
            ("/proj/sword.glb", ""),
            (
                "/proj/sword.glb.attr.toml",
                &fake_record_toml("LicenseRef-Mit"),
            ),
            ("/proj/_manifest.toml", &fake_record_toml("LicenseRef-Cc0")),
        ]);

        // When resolving.
        let r = resolve(&fs, Path::new("/proj/sword.glb"), Path::new("/proj")).unwrap();

        // Then the sidecar wins over the manifest.
        assert!(matches!(r.source, ResolutionSource::Sidecar(_)));
        assert_eq!(r.record.unwrap().license, "LicenseRef-Mit");
    }
}
