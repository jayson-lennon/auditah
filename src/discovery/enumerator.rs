//! Asset enumeration: walk a root, filter out excluded paths and metadata
//! files, return candidate asset paths for resolution.

use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};
use globset::{Glob, GlobSet, GlobSetBuilder};
use wherror::Error;

use crate::services::FsService;

/// Error building the exclude matcher or walking the filesystem.
#[derive(Debug, Error)]
#[error(debug)]
pub struct EnumerateError;

/// Compiled glob matcher for excluded paths.
#[derive(Debug, Clone)]
pub struct ExcludeMatcher {
    set: GlobSet,
}

impl ExcludeMatcher {
    /// Compile a list of glob patterns into a matcher.
    ///
    /// # Errors
    ///
    /// Returns an error if any pattern fails to compile.
    pub fn new(patterns: &[String]) -> Result<Self, Report<EnumerateError>> {
        let mut builder = GlobSetBuilder::new();
        for p in patterns {
            let glob = Glob::new(p)
                .change_context(EnumerateError)
                .attach(p.clone())?;
            builder.add(glob);
        }
        let set = builder
            .build()
            .change_context(EnumerateError)
            .attach("failed to compile exclude glob set")?;
        Ok(Self { set })
    }

    /// Whether a path (relative to root) is excluded.
    ///
    /// Matches against the forward-slash form of the relative path, and also
    /// against the bare file name, so patterns like `Cargo.toml` and
    /// `**/*.attr.toml` both work regardless of depth.
    #[must_use]
    pub fn is_excluded(&self, rel: &Path) -> bool {
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if self.set.is_match(&*rel_str) {
            return true;
        }
        // Also match the file name alone (e.g. pattern "Cargo.toml" at any depth).
        if let Some(name) = rel.file_name() {
            return self.set.is_match(name);
        }
        false
    }
}

/// Walks a root directory and returns candidate asset paths: files that are
/// not excluded and are not metadata (sidecars/manifests are excluded by the
/// default set, so this is a single filter pass).
///
/// # Errors
///
/// Returns an error if the filesystem walk fails.
pub fn enumerate(
    fs: &FsService,
    root: &Path,
    excludes: &ExcludeMatcher,
) -> Result<Vec<PathBuf>, Report<EnumerateError>> {
    let all = fs.walk(root).change_context(EnumerateError)?;
    Ok(filter_candidates(&all, root, excludes))
}

/// Pure filter: drop excluded paths. Sidecars and manifests are already covered
/// by the default exclude set, so no special-casing here.
fn filter_candidates(all: &[PathBuf], root: &Path, excludes: &ExcludeMatcher) -> Vec<PathBuf> {
    all.iter()
        .filter(|p| {
            let rel = p.strip_prefix(root).unwrap_or(p);
            !excludes.is_excluded(rel)
        })
        .cloned()
        .collect()
}

/// A directory's contents partitioned by kind, after applying excludes.
///
/// Built from a single typed listing so the cascade recurses into `dirs` and
/// audits `files` from one traversal of the directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirListing {
    /// Immediate subdirectory paths that are not excluded.
    pub dirs: Vec<PathBuf>,
    /// Immediate file paths that are not excluded.
    pub files: Vec<PathBuf>,
}

/// Partition a typed directory listing by kind, into (dirs, files).
///
/// Pure kind split: no exclude filtering, no metadata classification. The
/// cascade decides what to descend into and which files are assets vs
/// sidecars/manifests; this helper only separates directories from files
/// so each directory is traversed exactly once.
#[must_use]
pub fn partition_listing(entries: &[crate::services::DirEntry]) -> DirListing {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for e in entries {
        if e.is_dir {
            dirs.push(e.path.clone());
        } else {
            files.push(e.path.clone());
        }
    }
    DirListing { dirs, files }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeFs;
    use std::sync::Arc;

    fn fs_with(paths: &[&str]) -> FsService {
        FsService::new(Arc::new(FakeFs::with_files(paths.iter().map(|p| (*p, "")))))
    }

    #[test]
    fn exclude_matcher_matches_bare_filename_at_any_depth() {
        // Given an ExcludeMatcher with a bare filename pattern.
        let m = ExcludeMatcher::new(&["Cargo.toml".to_string()]).unwrap();

        // When checking the pattern at top-level.
        let top = m.is_excluded(Path::new("Cargo.toml"));

        // Then the filename matches at top-level.
        assert!(top);

        // When checking the pattern nested in a subdir.
        let nested = m.is_excluded(Path::new("sub/Cargo.toml"));

        // Then it matches nested too.
        assert!(nested);

        // When checking a non-matching path.
        let other = m.is_excluded(Path::new("src/main.rs"));

        // Then it is not excluded.
        assert!(!other);
    }

    #[test]
    fn enumerate_filters_excluded_paths() {
        // Given a fake filesystem with mixed assets and excluded files.
        let fs = fs_with(&[
            "/proj/assets/sword.glb",
            "/proj/Cargo.toml",
            "/proj/sword.glb.attr.toml",
            "/proj/assets/_manifest.toml",
            "/proj/target/debug/auditah",
        ]);
        let excludes = ExcludeMatcher::new(&crate::discovery::all_excludes(&[])).unwrap();

        // When enumerating with default excludes.
        let got = enumerate(&fs, Path::new("/proj"), &excludes).unwrap();
        let names: Vec<String> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        // Then only the non-excluded asset remains.
        assert!(names.contains(&"sword.glb".to_string()));
        assert!(!names.contains(&"Cargo.toml".to_string()));
        assert!(!names.contains(&"sword.glb.attr.toml".to_string()));
        assert!(!names.contains(&"_manifest.toml".to_string()));
        assert!(!names.iter().any(|n| n == "auditah"));
    }
}
