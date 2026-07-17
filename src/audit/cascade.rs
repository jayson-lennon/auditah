//! Cascade resolution: top-down descent of the asset tree.
//!
//! Replaces the previous up-walking resolver (`find_nearest_manifest` walked
//! each asset's ancestor chain calling `fs.exists` at every level). Instead,
//! the effective attribution record is *inherited downward*: the root carries
//! no record (or one placed at root), and each directory either keeps the
//! inherited record or fully replaces it via a local `_manifest.toml`
//! (full-replace semantics, by design). Per-asset `.attr.toml` sidecars
//! override the effective record for that one asset.
//!
//! Each directory is listed exactly once; that single listing feeds cascade
//! descent, per-asset resolution, and orphan detection. There is no global
//! walk and no ancestor walk (acceptance criterion: single traversal per
//! directory).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::audit::AuditError;
use crate::discovery::enumerator::{partition_listing, ExcludeMatcher};
use crate::discovery::resolver::{
    read_attribution, sidecar_path, strip_sidecar_suffix, ResolutionSource, ResolvedAsset,
    MANIFEST_FILENAME, SIDECAR_SUFFIX,
};
use crate::model::attribution::AttributionRecord;
use crate::services::FsService;

/// Manifest filename, re-exported so the cascade classifies entries by name
/// without reaching into the resolver module's internals.
#[derive(Debug, Error)]
#[error(debug)]
pub struct CascadeError;

/// The inherited payload: the manifest path the effective record came from, and
/// the record itself. Descends top-down; `None` at the root of an unlicensed
/// project (the reported scenario).
#[derive(Debug, Clone)]
pub struct Inherited {
    /// Path of the `_manifest.toml` the record originated from (rooted at the
    /// nearest ancestor manifest; carried so `ResolutionSource::Manifest`
    /// stays faithful without re-walking).
    pub manifest_path: PathBuf,
    /// The record inherited from that manifest.
    pub record: AttributionRecord,
}

/// The result of descending one directory: resolved assets, locally-detected
/// orphan sidecars, the effective record to pass into child directories, and
/// the child directories to recurse into.
#[derive(Debug, Clone)]
pub struct DirResult {
    /// One `ResolvedAsset` per non-excluded candidate asset in this directory.
    pub assets: Vec<ResolvedAsset>,
    /// Sidecars whose stripped asset name is not present in this directory's
    /// listing (locally detected — no global walk, no `fs.exists` probes).
    pub orphans: Vec<PathBuf>,
    /// The effective inherited payload to pass into each child directory:
    /// `Some` only if this directory (or an ancestor) established a record.
    pub effective: Option<Inherited>,
    /// Non-excluded child directories to recurse into.
    pub subdirs: Vec<PathBuf>,
}

/// One unit of audit work produced by the cascade and consumed by the auditor.
///
/// Either a resolved asset to check, or a locally-detected orphan sidecar.
#[derive(Debug, Clone)]
pub enum AuditInput {
    /// A resolved asset awaiting obligation checks.
    Asset(ResolvedAsset),
    /// A sidecar whose asset is absent in the same directory listing.
    Orphan(PathBuf),
}

/// Descend one directory: list it once, classify entries, resolve the local
/// manifest (full replace) or keep the inherited record, resolve each
/// candidate asset (sidecar override > effective inherited > uncovered), and
/// detect orphan sidecars from the same listing.
///
/// `root` is used to compute the relative path each entry is matched against
/// for exclusion. Excludes apply to candidate assets and to directory pruning
/// (a directory whose contents would all be excluded — e.g. `target/` — is
/// pruned). Metadata files (manifests, sidecars) are *not* excluded here; the
/// cascade needs to see them to resolve inheritance and detect orphans.
///
/// # Errors
///
/// Returns `AuditError` if the directory cannot be listed or a local
/// `_manifest.toml` exists but cannot be read or parsed. The caller emits a
/// `Verdict::Error` for the latter and does not descend the subtree.
pub fn descend(
    fs: &FsService,
    dir: &Path,
    root: &Path,
    excludes: &ExcludeMatcher,
    inherited: Option<Inherited>,
) -> Result<DirResult, Report<AuditError>> {
    let entries = fs
        .list_dir_typed(dir)
        .change_context(AuditError)
        .attach("failed to list directory for cascade")?;
    let listing = partition_listing(&entries);

    let local_manifest = local_manifest_path(&listing.files);
    let effective = resolve_effective(fs, local_manifest.as_deref(), inherited)?;

    let assets = resolve_assets(fs, &listing.files, effective.as_ref(), root, excludes);
    let orphans = detect_orphans(&listing.files);
    let subdirs = prune_subdirs(&listing.dirs, root, excludes);

    Ok(DirResult {
        assets,
        orphans,
        effective,
        subdirs,
    })
}

/// Locate the local `_manifest.toml` among a directory's files, if any.
fn local_manifest_path(files: &[PathBuf]) -> Option<PathBuf> {
    files
        .iter()
        .find(|f| f.file_name().is_some_and(|n| n == MANIFEST_FILENAME))
        .cloned()
}

/// Compute the effective record: a local manifest fully replaces the inherited
/// record (full-replace semantics); otherwise the inherited record passes
/// through unchanged.
///
/// # Errors
///
/// Returns `AuditError` if a local manifest exists but cannot be read/parsed.
fn resolve_effective(
    fs: &FsService,
    local_manifest: Option<&Path>,
    inherited: Option<Inherited>,
) -> Result<Option<Inherited>, Report<AuditError>> {
    match local_manifest {
        Some(path) => {
            let record = read_attribution(fs, path)
                .change_context(AuditError)
                .attach("failed to read local manifest")?;
            Ok(Some(Inherited {
                manifest_path: path.to_path_buf(),
                record,
            }))
        }
        None => Ok(inherited),
    }
}

/// Resolve every candidate asset in the listing. A candidate is a file that is
/// not metadata (not a manifest, not a sidecar) and not excluded. Precedence
/// per asset: sidecar override > effective inherited > uncovered.
fn resolve_assets(
    fs: &FsService,
    files: &[PathBuf],
    effective: Option<&Inherited>,
    root: &Path,
    excludes: &ExcludeMatcher,
) -> Vec<ResolvedAsset> {
    files
        .iter()
        .filter(|f| !is_metadata(f))
        .filter(|f| !is_excluded(f, root, excludes))
        .map(|asset| resolve_one(fs, asset, effective))
        .collect()
}

/// Resolve a single asset: sidecar > effective > uncovered.
fn resolve_one(fs: &FsService, asset: &Path, effective: Option<&Inherited>) -> ResolvedAsset {
    let sidecar = sidecar_path(asset);
    if fs.exists(&sidecar) {
        // Sidecar read errors are surfaced at audit time via the record the
        // auditor validates; the cascade treats an unreadable sidecar as
        // "present but unresolved" and lets the auditor raise UnknownLicense.
        // ResolutionSource still marks it as Sidecar so the finding lands.
        return match read_attribution(fs, &sidecar) {
            Ok(record) => ResolvedAsset {
                asset_path: asset.to_path_buf(),
                record: Some(record),
                source: ResolutionSource::Sidecar(sidecar),
            },
            Err(_) => ResolvedAsset {
                asset_path: asset.to_path_buf(),
                record: None,
                source: ResolutionSource::None,
            },
        };
    }
    match effective {
        Some(inh) => ResolvedAsset {
            asset_path: asset.to_path_buf(),
            record: Some(inh.record.clone()),
            source: ResolutionSource::Manifest(inh.manifest_path.clone()),
        },
        None => ResolvedAsset {
            asset_path: asset.to_path_buf(),
            record: None,
            source: ResolutionSource::None,
        },
    }
}

/// Detect orphan sidecars from the directory's own listing: a sidecar whose
/// stripped asset name is not a file in this same listing. Fully local — no
/// `fs.exists`, no global walk.
fn detect_orphans(files: &[PathBuf]) -> Vec<PathBuf> {
    let names: HashSet<PathBuf> = files.iter().cloned().collect();
    files
        .iter()
        .filter(|f| is_sidecar(f))
        .filter(|sidecar| !names.contains(&strip_sidecar_suffix(sidecar)))
        .cloned()
        .collect()
}

/// Prune excluded subdirectories. A directory is pruned if the directory path
/// itself matches an exclude glob, OR if a probe file beneath it matches (so
/// `**/target/**`-style globs prune the whole subtree rather than letting the
/// cascade descend into it only to drop everything).
fn prune_subdirs(dirs: &[PathBuf], root: &Path, excludes: &ExcludeMatcher) -> Vec<PathBuf> {
    dirs.iter()
        .filter(|d| !dir_is_excluded(d, root, excludes))
        .cloned()
        .collect()
}

/// Whether a directory is excluded: the directory path itself matches an
/// exclude glob, or a representative probe file beneath it would.
fn dir_is_excluded(dir: &Path, root: &Path, excludes: &ExcludeMatcher) -> bool {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    if excludes.is_excluded(rel) {
        return true;
    }
    // Probe a hypothetical child so `**/target/**` matches the `target/` dir
    // by its contents; a bare-name glob (e.g. `Cargo.toml`) must NOT prune a
    // whole directory just because it could match a file inside.
    let probe = rel.join("__probe__");
    let probe_matches = excludes.is_excluded(&probe);
    let dir_glob = format!("{}/**", rel.to_string_lossy().replace('\\', "/"));
    probe_matches && excludes.is_excluded(Path::new(&dir_glob))
}

/// A file is metadata if it is the directory manifest or an attribution sidecar.
fn is_metadata(path: &Path) -> bool {
    is_manifest(path) || is_sidecar(path)
}

fn is_manifest(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == MANIFEST_FILENAME)
}

fn is_sidecar(path: &Path) -> bool {
    path.to_string_lossy().ends_with(SIDECAR_SUFFIX)
}

/// Apply the exclude matcher to a candidate asset (relative to root).
fn is_excluded(path: &Path, root: &Path, excludes: &ExcludeMatcher) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    excludes.is_excluded(rel)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::all_excludes;
    use crate::test_support::FakeFs;
    use std::sync::Arc;

    fn fs_with(files: &[(&str, &str)]) -> FsService {
        FsService::new(Arc::new(FakeFs::with_files(
            files.iter().map(|(p, c)| (*p, *c)),
        )))
    }

    fn default_excludes() -> ExcludeMatcher {
        ExcludeMatcher::new(&all_excludes(&[])).unwrap()
    }

    fn record_toml(license: &str) -> String {
        format!(
            r#"title = "T"
author = "A"
year = 2020
license = "{license}"
source = "https://example.com"
"#
        )
    }

    fn inherited(license: &str) -> Inherited {
        Inherited {
            manifest_path: PathBuf::from("/proj/_manifest.toml"),
            record: toml::from_str(&record_toml(license)).unwrap(),
        }
    }

    #[test]
    fn deep_nested_asset_inherits_ancestor_record() {
        // Given a manifest at root and an asset nested several levels deep.
        let fs = fs_with(&[
            ("/proj/_manifest.toml", &record_toml("LicenseRef-CcBy")),
            ("/proj/a/b/c/sword.glb", ""),
        ]);
        let excludes = default_excludes();

        // When descending the root with no inherited record.
        let root_result =
            descend(&fs, Path::new("/proj"), Path::new("/proj"), &excludes, None).unwrap();

        // Then the effective record is established for descent.
        assert!(root_result.effective.is_some());
        // And the nested asset is not resolved at the root (it lives deeper).
        assert!(root_result.assets.is_empty());
        // And the root has one subdir to recurse into.
        assert_eq!(root_result.subdirs.len(), 1);
    }

    #[test]
    fn child_manifest_fully_replaces_inherited() {
        // Given a parent manifest and a child manifest with a different license.
        let fs = fs_with(&[
            ("/proj/sub/_manifest.toml", &record_toml("LicenseRef-Mit")),
            ("/proj/sub/sword.glb", ""),
        ]);
        let excludes = default_excludes();

        // When descending the child with an inherited CC-BY record.
        let result = descend(
            &fs,
            Path::new("/proj/sub"),
            Path::new("/proj"),
            &excludes,
            Some(inherited("LicenseRef-CcBy")),
        )
        .unwrap();

        // Then the child manifest fully replaces the inherited record.
        let resolved = &result.assets[0];
        assert!(matches!(resolved.source, ResolutionSource::Manifest(_)));
        assert_eq!(resolved.record.as_ref().unwrap().license, "LicenseRef-Mit");
    }

    #[test]
    fn sidecar_overrides_inherited_manifest() {
        // Given an asset whose sidecar declares a different license than inherited.
        let fs = fs_with(&[
            ("/proj/sword.glb", ""),
            ("/proj/sword.glb.attr.toml", &record_toml("LicenseRef-Mit")),
        ]);
        let excludes = default_excludes();

        // When descending with an inherited CC-BY record.
        let result = descend(
            &fs,
            Path::new("/proj"),
            Path::new("/proj"),
            &excludes,
            Some(inherited("LicenseRef-CcBy")),
        )
        .unwrap();

        // Then the sidecar overrides the inherited manifest.
        let resolved = &result.assets[0];
        assert!(matches!(resolved.source, ResolutionSource::Sidecar(_)));
        assert_eq!(resolved.record.as_ref().unwrap().license, "LicenseRef-Mit");
    }

    #[test]
    fn orphan_sidecar_detected_locally_when_asset_absent() {
        // Given a real asset+sidecar and a ghost sidecar with no asset.
        let fs = fs_with(&[
            ("/proj/real.glb", ""),
            ("/proj/real.glb.attr.toml", ""),
            ("/proj/ghost.glb.attr.toml", ""),
        ]);
        let excludes = default_excludes();

        // When descending.
        let result = descend(&fs, Path::new("/proj"), Path::new("/proj"), &excludes, None).unwrap();

        // Then only the ghost sidecar is reported as an orphan.
        assert_eq!(result.orphans.len(), 1);
        assert!(result.orphans[0].to_string_lossy().contains("ghost"));
    }

    #[test]
    fn asset_with_no_cascade_is_uncovered() {
        // Given an asset with no sidecar and no inherited record.
        let fs = fs_with(&[("/proj/sword.glb", "")]);
        let excludes = default_excludes();

        // When descending with no inherited record.
        let result = descend(&fs, Path::new("/proj"), Path::new("/proj"), &excludes, None).unwrap();

        // Then the asset is uncovered.
        let resolved = &result.assets[0];
        assert_eq!(resolved.source, ResolutionSource::None);
        assert!(resolved.record.is_none());
    }

    #[test]
    fn malformed_manifest_returns_error() {
        // Given a directory whose manifest is malformed TOML.
        let fs = fs_with(&[("/proj/_manifest.toml", "not = = valid toml")]);
        let excludes = default_excludes();

        // When descending.
        let result = descend(&fs, Path::new("/proj"), Path::new("/proj"), &excludes, None);

        // Then descent returns an error (caller emits Verdict::Error, skips subtree).
        assert!(result.is_err());
    }

    #[test]
    fn descend_lists_each_directory_exactly_once() {
        // Given a nested tree where the root and two subdirs each have assets.
        let manifest = record_toml("LicenseRef-CcBy");
        let fs = Arc::new(FakeFs::with_files([
            (Path::new("/proj/_manifest.toml"), manifest.as_str()),
            (Path::new("/proj/a.glb"), ""),
            (Path::new("/proj/sub/b.glb"), ""),
            (Path::new("/proj/sub/deep/c.glb"), ""),
        ]));
        let excludes = default_excludes();
        let service = FsService::new(fs.clone());

        // When descending the root, then its child, then the grandchild,
        // passing the effective record down at each level.
        let r0 = descend(
            &service,
            Path::new("/proj"),
            Path::new("/proj"),
            &excludes,
            None,
        )
        .unwrap();
        let r1 = descend(
            &service,
            &r0.subdirs[0],
            Path::new("/proj"),
            &excludes,
            r0.effective.clone(),
        )
        .unwrap();
        let r2 = descend(
            &service,
            &r1.subdirs[0],
            Path::new("/proj"),
            &excludes,
            r1.effective.clone(),
        )
        .unwrap();

        // Then every listed directory was hit exactly once — no re-walks,
        // no ancestor probes, no double traversal (acceptance criterion 7).
        assert_eq!(fs.list_dir_call_count(Path::new("/proj")), 1);
        assert_eq!(fs.list_dir_call_count(&r0.subdirs[0]), 1);
        assert_eq!(fs.list_dir_call_count(&r1.subdirs[0]), 1);
        // And the cascade reached the deep asset by inheritance alone.
        assert_eq!(r2.assets.len(), 1);
    }
}
