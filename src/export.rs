//! Asset export: copy an asset (file or directory) from a library to a target
//! location, carrying its attribution with it.
//!
//! Export is the bridge between a shared asset **library** (its own auditah
//! project with a `LICENSES/` directory) and per-game project directories.
//! Copying an asset by hand leaves attribution behind in the library; export
//! copies the asset **and** the attribution that travels with it:
//!
//! - **file → file**: the asset is copied, and a `<target>.attr.toml` sidecar
//!   is written. If the source had an adjacent sidecar it is copied; if the
//!   source was only covered by an ancestor `_manifest.toml`, a new sidecar is
//!   synthesized from that manifest (with the title renamed to signal the
//!   asset's provenance from the pack).
//! - **dir → dir**: the directory tree is copied verbatim — including nested
//!   `_manifest.toml` files and per-file `.attr.toml` sidecars — and the target
//!   root is guaranteed to have a `_manifest.toml`.
//!
//! The target project is never read, loaded, or modified: it is a sink. A
//! post-export `auditah audit` in the target may still fail until the user
//! provisions the referenced licenses there (`license provision`); that is
//! expected and intentional.
//!
//! The audit gate (run by the command, not here) guarantees every successfully
//! exported asset had resolvable attribution at the source, so no fallback
//! blank attribution is ever written.

use std::path::Path;

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::discovery::resolver::{self, sidecar_path, ResolutionSource};
use crate::services::Services;

/// Error during an export operation.
#[derive(Debug, Error)]
#[error(debug)]
pub struct ExportError;

/// Copy a single asset file from `source_file` to `target_file`, carrying its
/// attribution.
///
/// The target always ends up with a sidecar (`<target_file>.attr.toml`):
/// - If the source has an adjacent sidecar, it is copied verbatim alongside.
/// - If the source is covered only by an ancestor `_manifest.toml`, a sidecar
///   is synthesized from that manifest with its title renamed to
///   `<file stem> (from <pack>)`.
///
/// # Errors
///
/// Returns `ExportError` if the source is uncovered (no sidecar and no
/// enclosing manifest), or if any read/copy/write fails.
pub fn export_file(
    services: &Services,
    source_file: &Path,
    target_file: &Path,
    _copy_ignored: bool,
) -> Result<(), Report<ExportError>> {
    let resolved = resolver::resolve(services, source_file, services.config.root())
        .change_context(ExportError)
        .attach("failed to resolve source attribution")?;

    match resolved.source {
        ResolutionSource::Sidecar(source_sidecar) => {
            services
                .fs
                .copy_file(source_file, target_file)
                .change_context(ExportError)
                .attach("failed to copy asset file")?;
            services
                .fs
                .copy_file(&source_sidecar, &sidecar_path(target_file))
                .change_context(ExportError)
                .attach("failed to copy source sidecar")
        }
        ResolutionSource::Manifest(manifest_path) => {
            services
                .fs
                .copy_file(source_file, target_file)
                .change_context(ExportError)
                .attach("failed to copy asset file")?;
            let record = resolved.record.ok_or_else(|| Report::new(ExportError))?;
            let renamed = with_provenance_title(&record, source_file, &manifest_path);
            crate::add::write_sidecar(services, target_file, &renamed)
                .change_context(ExportError)
                .attach("failed to write synthesized sidecar")
        }
        ResolutionSource::None => Err(Report::new(ExportError))
            .attach("source file is uncovered (no sidecar and no enclosing manifest)")
            .attach(source_file.display().to_string()),
    }
}

/// Build a copy of `record` with its title replaced by
/// `<source file stem> (from <manifest dir name>)`.
///
/// Manifest titles describe the whole pack (e.g. "Nature Pack"), which is wrong
/// for an individual exported file. The provenance form signals both the file
/// and its originating pack.
fn with_provenance_title(
    record: &crate::model::attribution::AttributionRecord,
    source_file: &Path,
    manifest_path: &Path,
) -> crate::model::attribution::AttributionRecord {
    let mut renamed = record.clone();
    renamed.title = provenance_title(source_file, manifest_path);
    renamed
}

/// Format the provenance title: `<stem> (from <pack>)`, where `<pack>` is the
/// manifest's directory name. Falls back gracefully when names are absent.
#[must_use]
pub(crate) fn provenance_title(source_file: &Path, manifest_path: &Path) -> String {
    let stem = source_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let pack = manifest_path
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    format!("{stem} (from {pack})")
}

/// Copy a directory tree from `source_dir` to `target_dir`, verbatim, carrying
/// attribution.
///
/// All non-skipped files are copied, mirroring the source structure. Sidecars
/// (`*.attr.toml`) and manifests (`_manifest.toml`) are ALWAYS copied,
/// regardless of the exclude matcher. Other files are copied unless the merged
/// exclude matcher excludes them and `copy_ignored` is false. The target root
/// is guaranteed to end up with a `_manifest.toml`.
///
/// # Errors
///
/// Returns `ExportError` if a directory listing, copy, or synthesized manifest
/// write fails.
pub fn export_dir(
    services: &Services,
    source_dir: &Path,
    target_dir: &Path,
    copy_ignored: bool,
) -> Result<(), Report<ExportError>> {
    let matcher = build_matcher(services)?;
    copy_tree(services, source_dir, target_dir, &matcher, copy_ignored)?;
    ensure_target_manifest(services, source_dir, target_dir)
}

/// Build the merged exclude matcher (default excludes + user config excludes).
fn build_matcher(
    services: &Services,
) -> Result<crate::discovery::enumerator::ExcludeMatcher, Report<ExportError>> {
    let patterns = crate::discovery::all_excludes(&services.config.config().exclude);
    crate::discovery::enumerator::ExcludeMatcher::new(&patterns)
        .change_context(ExportError)
        .attach("failed to compile exclude matcher")
}

/// Recursively copy `source_dir` into `target_dir`, mirroring structure.
///
/// One loop, descending into subdirectories via this same function.
fn copy_tree(
    services: &Services,
    source_dir: &Path,
    target_dir: &Path,
    matcher: &crate::discovery::enumerator::ExcludeMatcher,
    copy_ignored: bool,
) -> Result<(), Report<ExportError>> {
    services
        .fs
        .create_dir_all(target_dir)
        .change_context(ExportError)
        .attach("failed to create target directory")?;

    let root = services.config.root();
    let entries = services
        .fs
        .list_dir_typed(source_dir)
        .change_context(ExportError)
        .attach("failed to list source directory")?;

    for entry in entries {
        copy_entry(services, &entry, target_dir, root, matcher, copy_ignored)?;
    }
    Ok(())
}

/// Copy a single directory entry (file or subdir) according to export rules.
fn copy_entry(
    services: &Services,
    entry: &crate::services::fs::DirEntry,
    target_dir: &Path,
    root: &Path,
    matcher: &crate::discovery::enumerator::ExcludeMatcher,
    copy_ignored: bool,
) -> Result<(), Report<ExportError>> {
    let abs_source = &entry.path;
    let rel = abs_source.strip_prefix(root).unwrap_or(abs_source);
    let abs_target = target_dir.join(entry.path.file_name().unwrap_or_default());

    if entry.is_dir {
        // Recurse into subdirectories unconditionally (the matcher is applied
        // per-file inside the recursion; a directory itself is never excluded).
        return copy_tree(services, abs_source, &abs_target, matcher, copy_ignored);
    }

    if is_attribution_file(abs_source) {
        // Sidecars and manifests ALWAYS travel with the export.
        return services
            .fs
            .copy_file(abs_source, &abs_target)
            .change_context(ExportError)
            .attach("failed to copy attribution file");
    }

    if !copy_ignored && matcher.is_excluded(rel) {
        return Ok(());
    }

    services
        .fs
        .copy_file(abs_source, &abs_target)
        .change_context(ExportError)
        .attach("failed to copy asset file")
}

/// Whether `path` is an attribution metadata file (sidecar or manifest).
fn is_attribution_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    name == resolver::MANIFEST_FILENAME || name.ends_with(resolver::SIDECAR_SUFFIX)
}

/// Ensure the target root has a `_manifest.toml`.
///
/// If the source dir had its own manifest, the copy step already mirrored it
/// into the target root → nothing to do. Otherwise the source dir was covered
/// by a nearest ANCESTOR manifest; synthesize one at the target root from it.
/// If there is no enclosing manifest at all, the audit gate should have already
/// aborted — here we surface a defense-in-depth error.
fn ensure_target_manifest(
    services: &Services,
    source_dir: &Path,
    target_dir: &Path,
) -> Result<(), Report<ExportError>> {
    let target_manifest = target_dir.join(resolver::MANIFEST_FILENAME);
    if services.fs.exists(&target_manifest) {
        return Ok(());
    }

    let ancestor = resolver::find_nearest_manifest(services, source_dir, services.config.root())
        .ok_or_else(|| {
            Report::new(ExportError)
                .attach("source directory has no manifest and no enclosing manifest")
                .attach(source_dir.display().to_string())
        })?;
    let record = resolver::read_attribution(services, &ancestor)
        .change_context(ExportError)
        .attach("failed to read enclosing manifest")?;
    crate::add::write_manifest(services, target_dir, &record)
        .change_context(ExportError)
        .attach("failed to write synthesized target manifest")
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::model::attribution::AttributionRecord;
    use crate::services::fs::FsService;
    use crate::services::Services;
    use crate::test_support::FakeFs;
    use std::sync::Arc;

    // Attribution record serialized as it appears in a sidecar or manifest.
    fn record_toml(title: &str, license: &str) -> String {
        format!("title = \"{title}\"\nauthor = \"A\"\nyear = 2020\nlicense = \"{license}\"\nsource = \"https://example.com\"\n")
    }

    // Services rooted at `/proj` with a seeded FakeFs.
    fn services_with(files: &[(&str, &str)]) -> Services {
        let fs = FsService::new(Arc::new(FakeFs::with_files(
            files.iter().map(|(p, c)| (*p, *c)),
        )));
        Services::test()
            .fs(fs)
            .config_root(Path::new("/proj"), Config::default())
            .build()
    }

    // Parse a written sidecar/manifest back into an AttributionRecord.
    fn read_record(services: &Services, path: &Path) -> AttributionRecord {
        toml::from_str(&services.fs.read_to_string(path).unwrap()).unwrap()
    }

    // --- export_file: sidecar travels with the file ---

    #[test]
    fn export_file_copies_asset_and_adjacent_sidecar_to_target() {
        // Given a source file with its own sidecar.
        let services = services_with(&[
            ("/proj/sword.glb", "asset-bytes"),
            (
                "/proj/sword.glb.attr.toml",
                &record_toml("Sword", "LicenseRef-Mit"),
            ),
        ]);

        // When exporting the file to a new location.
        export_file(
            &services,
            Path::new("/proj/sword.glb"),
            Path::new("/game/sword.glb"),
            false,
        )
        .expect("export");

        // Then the target file and a sidecar with the source license exist.
        assert_eq!(
            services
                .fs
                .read_to_string(Path::new("/game/sword.glb"))
                .unwrap(),
            "asset-bytes"
        );
        let rec = read_record(&services, Path::new("/game/sword.glb.attr.toml"));
        assert_eq!(rec.license, "LicenseRef-Mit");
    }

    // --- export_file: ancestor-manifest coverage renames the title ---

    #[test]
    fn export_file_synthesizes_sidecar_with_provenance_title_from_manifest() {
        // Given a source file covered only by an ancestor manifest.
        let services = services_with(&[
            ("/proj/assets/sword.glb", "asset-bytes"),
            (
                "/proj/_manifest.toml",
                &record_toml("Nature Pack", "LicenseRef-Cc0"),
            ),
        ]);

        // When exporting the file to a new location.
        export_file(
            &services,
            Path::new("/proj/assets/sword.glb"),
            Path::new("/game/sword.glb"),
            false,
        )
        .expect("export");

        // Then the target sidecar title is '<stem> (from <pack dir>)'.
        let rec = read_record(&services, Path::new("/game/sword.glb.attr.toml"));
        assert_eq!(rec.title, "sword (from proj)");
        assert_eq!(rec.license, "LicenseRef-Cc0");
    }

    // --- export_file: uncovered source errors ---

    #[test]
    fn export_file_errors_when_source_has_no_attribution() {
        // Given a source file with no sidecar and no enclosing manifest.
        let services = services_with(&[("/proj/sword.glb", "asset-bytes")]);

        // When exporting the uncovered file.
        let result = export_file(
            &services,
            Path::new("/proj/sword.glb"),
            Path::new("/game/sword.glb"),
            false,
        );

        // Then it surfaces an error and writes nothing.
        assert!(result.is_err());
        assert!(!services.fs.exists(Path::new("/game/sword.glb")));
    }

    // --- export_dir: local manifest travels with the tree ---

    #[test]
    fn export_dir_with_local_manifest_copies_it_to_target_root() {
        // Given a source dir with its own _manifest.toml.
        let services = services_with(&[
            ("/proj/pack/tree.glb", "tree"),
            (
                "/proj/pack/_manifest.toml",
                &record_toml("Nature Pack", "LicenseRef-Cc0"),
            ),
        ]);

        // When exporting the directory.
        export_dir(
            &services,
            Path::new("/proj/pack"),
            Path::new("/game/pack"),
            false,
        )
        .expect("export");

        // Then the target root has the manifest and the asset is copied.
        assert!(services.fs.exists(Path::new("/game/pack/_manifest.toml")));
        assert_eq!(
            services
                .fs
                .read_to_string(Path::new("/game/pack/tree.glb"))
                .unwrap(),
            "tree"
        );
    }

    // --- export_dir: ancestor-manifest coverage synthesizes a target manifest ---

    #[test]
    fn export_dir_synthesizes_manifest_at_target_root_from_ancestor() {
        // Given a source dir covered only by an ancestor manifest.
        let services = services_with(&[
            ("/proj/pack/sub/tree.glb", "tree"),
            (
                "/proj/_manifest.toml",
                &record_toml("Nature Pack", "LicenseRef-Cc0"),
            ),
        ]);

        // When exporting the subdirectory.
        export_dir(
            &services,
            Path::new("/proj/pack"),
            Path::new("/game/pack"),
            false,
        )
        .expect("export");

        // Then the target root gains a synthesized _manifest.toml.
        let rec = read_record(&services, Path::new("/game/pack/_manifest.toml"));
        assert_eq!(rec.license, "LicenseRef-Cc0");
    }

    // --- export_dir: nested subdir manifests are preserved ---

    #[test]
    fn export_dir_preserves_nested_subdir_manifests() {
        // Given a source dir with a nested subdir manifest.
        let services = services_with(&[
            (
                "/proj/pack/_manifest.toml",
                &record_toml("Outer", "LicenseRef-Cc0"),
            ),
            ("/proj/pack/sub/tree.glb", "tree"),
            (
                "/proj/pack/sub/_manifest.toml",
                &record_toml("Inner", "LicenseRef-Mit"),
            ),
        ]);

        // When exporting the directory.
        export_dir(
            &services,
            Path::new("/proj/pack"),
            Path::new("/game/pack"),
            false,
        )
        .expect("export");

        // Then both the root and nested subdir manifests exist in the target.
        let outer = read_record(&services, Path::new("/game/pack/_manifest.toml"));
        let inner = read_record(&services, Path::new("/game/pack/sub/_manifest.toml"));
        assert_eq!(outer.license, "LicenseRef-Cc0");
        assert_eq!(inner.license, "LicenseRef-Mit");
    }

    // --- export_dir: default-skip of matcher-excluded files ---

    #[test]
    fn export_dir_skips_excluded_files_by_default() {
        // Given a source dir with a build-output file (matched by DEFAULT_EXCLUDES).
        let services = services_with(&[
            ("/proj/pack/tree.glb", "tree"),
            (
                "/proj/pack/_manifest.toml",
                &record_toml("Pack", "LicenseRef-Cc0"),
            ),
            ("/proj/pack/target/build.o", "junk"),
        ]);

        // When exporting the directory with default flags.
        export_dir(
            &services,
            Path::new("/proj/pack"),
            Path::new("/game/pack"),
            false,
        )
        .expect("export");

        // Then the excluded build file is not copied to the target.
        assert!(!services.fs.exists(Path::new("/game/pack/target/build.o")));
    }

    // --- export_dir: --copy-ignored copies matcher-excluded files ---

    #[test]
    fn export_dir_copy_ignored_copies_excluded_files() {
        // Given a source dir with a build-output file.
        let services = services_with(&[
            ("/proj/pack/tree.glb", "tree"),
            (
                "/proj/pack/_manifest.toml",
                &record_toml("Pack", "LicenseRef-Cc0"),
            ),
            ("/proj/pack/target/build.o", "junk"),
        ]);

        // When exporting with copy_ignored = true.
        export_dir(
            &services,
            Path::new("/proj/pack"),
            Path::new("/game/pack"),
            true,
        )
        .expect("export");

        // Then the otherwise-excluded file is copied.
        assert!(services.fs.exists(Path::new("/game/pack/target/build.o")));
    }

    // --- export_dir: sidecars are always copied (override the matcher) ---

    #[test]
    fn export_dir_always_copies_sidecars_despite_matcher() {
        // Given a source dir with an asset and its sidecar.
        let services = services_with(&[
            ("/proj/pack/tree.glb", "tree"),
            (
                "/proj/pack/tree.glb.attr.toml",
                &record_toml("Tree", "LicenseRef-Mit"),
            ),
            (
                "/proj/pack/_manifest.toml",
                &record_toml("Pack", "LicenseRef-Cc0"),
            ),
        ]);

        // When exporting the directory.
        export_dir(
            &services,
            Path::new("/proj/pack"),
            Path::new("/game/pack"),
            false,
        )
        .expect("export");

        // Then the sidecar (which DEFAULT_EXCLUDES normally skips) is copied.
        assert!(services
            .fs
            .exists(Path::new("/game/pack/tree.glb.attr.toml")));
    }

    // --- export_dir: LICENSES/ subtree is skipped by default ---

    #[test]
    fn export_dir_skips_licenses_subtree_by_default() {
        // Given a source dir containing a nested LICENSES directory.
        let services = services_with(&[
            ("/proj/pack/tree.glb", "tree"),
            (
                "/proj/pack/_manifest.toml",
                &record_toml("Pack", "LicenseRef-Cc0"),
            ),
            ("/proj/pack/LICENSES/MIT.txt", "license text"),
        ]);

        // When exporting the directory with default flags.
        export_dir(
            &services,
            Path::new("/proj/pack"),
            Path::new("/game/pack"),
            false,
        )
        .expect("export");

        // Then the LICENSES file is not copied to the target.
        assert!(!services.fs.exists(Path::new("/game/pack/LICENSES/MIT.txt")));
    }

    #[test]
    fn provenance_title_combines_stem_and_manifest_dir() {
        // Given a file and its enclosing manifest path.
        // When formatting the provenance title.
        let title = provenance_title(
            Path::new("/lib/nature/tree.glb"),
            Path::new("/lib/nature/_manifest.toml"),
        );

        // Then the title is '<stem> (from <manifest dir name>)'.
        assert_eq!(title, "tree (from nature)");
    }
}
