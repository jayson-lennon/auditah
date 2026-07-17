//! Filesystem service: abstraction over file reads/writes/walks so that
//! core logic is testable without a real filesystem.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use derive_more::Debug;
use error_stack::{Report, ResultExt};
use wherror::Error;

/// Error type for filesystem operations. Colocated with [`FsBackend`] per the
/// service-trait pattern.
#[derive(Debug, Error)]
#[error(debug)]
pub struct FsError;

/// A single entry within a directory listing, with its kind.
///
/// Returned by [`FsBackend::list_dir_typed`] so callers can recurse into
/// subdirectories without a separate `exists`/stat per entry. The untyped
/// [`FsBackend::list_dir`] is retained for callers that only want paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// Absolute path to the entry.
    pub path: PathBuf,
    /// Whether this entry is a directory (vs a file).
    pub is_dir: bool,
}

impl DirEntry {
    /// Construct a directory entry.
    #[must_use]
    pub fn dir(path: PathBuf) -> Self {
        Self { path, is_dir: true }
    }

    /// Construct a file entry.
    #[must_use]
    pub fn file(path: PathBuf) -> Self {
        Self {
            path,
            is_dir: false,
        }
    }
}

/// Capability trait: read/write/list/walk the filesystem.
///
/// Production uses [`RealFs`]; tests use a fake in-memory backend.
pub trait FsBackend: Send + Sync {
    /// Read a file's full contents as a UTF-8 string.
    ///
    /// # Errors
    /// Returns [`FsError`] if the file cannot be read or is not valid UTF-8.
    fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>>;

    /// Write `content` to `path`, creating parent directories as needed.
    ///
    /// # Errors
    /// Returns [`FsError`] if the write fails.
    fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>>;

    /// Create the directory at `path`, including parents. Idempotent: a no-op
    /// when the directory already exists.
    ///
    /// # Errors
    /// Returns [`FsError`] if the directory cannot be created.
    fn create_dir_all(&self, path: &Path) -> Result<(), Report<FsError>>;

    /// List immediate children (files + dirs) of `path`, untyped.
    ///
    /// # Errors
    /// Returns [`FsError`] if the directory cannot be read.
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;

    /// List immediate children of `path` with their kind (file vs dir).
    ///
    /// This is the directory-recursion primitive: a single listing yields both
    /// the files to audit and the subdirectories to descend into. Each directory
    /// is therefore traversed exactly once.
    ///
    /// # Errors
    /// Returns [`FsError`] if the directory cannot be read.
    fn list_dir_typed(&self, path: &Path) -> Result<Vec<DirEntry>, Report<FsError>>;

    /// Recursively walk `root`, returning every file path beneath it.
    ///
    /// # Errors
    /// Returns [`FsError`] if the walk fails.
    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;

    /// Whether `path` exists on the backing filesystem.
    fn exists(&self, path: &Path) -> bool;

    /// Copy a file from `src` to `dst`, binary-safe.
    ///
    /// Unlike [`read_to_string`](Self::read_to_string) + [`write`](Self::write),
    /// this preserves exact bytes (no UTF-8 round-trip) so binary assets
    /// (`.glb`, `.png`, `.wav`) move faithfully. Creates `dst`'s parent
    /// directories as needed.
    ///
    /// # Errors
    /// Returns [`FsError`] if `src` cannot be read or `dst` cannot be written.
    fn copy_file(&self, src: &Path, dst: &Path) -> Result<(), Report<FsError>>;

    /// Backend name for debugging.
    fn name(&self) -> &'static str;
}

/// Shared, cloneable wrapper around an [`FsBackend`] trait object.
#[derive(Debug, Clone)]
pub struct FsService {
    #[debug("FsService<{}>", self.backend.name())]
    backend: Arc<dyn FsBackend>,
}

impl FsService {
    /// Wrap a backend. The service is cheap to clone (one [`Arc`] refcount).
    #[must_use]
    pub fn new(backend: Arc<dyn FsBackend>) -> Self {
        Self { backend }
    }

    /// Read a file as a UTF-8 string. See [`FsBackend::read_to_string`].
    ///
    /// # Errors
    /// Propagates [`FsError`] from the backend, with the path attached as context.
    pub fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>> {
        self.backend
            .read_to_string(path)
            .attach(path.display().to_string())
            .attach("failed to read file")
    }

    /// Write content to a file. See [`FsBackend::write`].
    ///
    /// # Errors
    /// Propagates [`FsError`] from the backend, with the path attached as context.
    pub fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>> {
        self.backend
            .write(path, content)
            .attach(path.display().to_string())
            .attach("failed to write file")
    }

    /// Create a directory (and parents). See [`FsBackend::create_dir_all`].
    ///
    /// # Errors
    /// Propagates [`FsError`] from the backend, with the path attached as context.
    pub fn create_dir_all(&self, path: &Path) -> Result<(), Report<FsError>> {
        self.backend
            .create_dir_all(path)
            .attach(path.display().to_string())
            .attach("failed to create directory")
    }

    /// List a directory. See [`FsBackend::list_dir`].
    ///
    /// # Errors
    /// Propagates [`FsError`] from the backend.
    pub fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        self.backend
            .list_dir(path)
            .attach(path.display().to_string())
            .attach("failed to list directory")
    }

    /// List a directory with entry kinds (file vs dir). See [`FsBackend::list_dir_typed`].
    ///
    /// # Errors
    /// Propagates [`FsError`] from the backend.
    pub fn list_dir_typed(&self, path: &Path) -> Result<Vec<DirEntry>, Report<FsError>> {
        self.backend
            .list_dir_typed(path)
            .attach(path.display().to_string())
            .attach("failed to list directory")
    }

    /// Recursively walk a root. See [`FsBackend::walk`].
    ///
    /// # Errors
    /// Propagates [`FsError`] from the backend.
    pub fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        self.backend
            .walk(root)
            .attach(root.display().to_string())
            .attach("failed to walk directory")
    }

    /// Whether a path exists. See [`FsBackend::exists`].
    #[must_use]
    pub fn exists(&self, path: &Path) -> bool {
        self.backend.exists(path)
    }

    /// Copy `src` to `dst`, binary-safe. See [`FsBackend::copy_file`].
    ///
    /// # Errors
    /// Propagates [`FsError`] from the backend, with both paths attached as context.
    pub fn copy_file(&self, src: &Path, dst: &Path) -> Result<(), Report<FsError>> {
        self.backend
            .copy_file(src, dst)
            .attach(src.display().to_string())
            .attach(dst.display().to_string())
            .attach("failed to copy file")
    }
}

/// Production [`FsBackend`] backed by the real filesystem via `std::fs`
/// and [`walkdir`]. Construct via [`RealFs::new`] and wrap in [`FsService`].
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl RealFs {
    /// Create a new real-filesystem backend.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl FsBackend for RealFs {
    fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>> {
        std::fs::read_to_string(path)
            .change_context(FsError)
            .attach(path.display().to_string())
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .change_context(FsError)
                .attach(parent.display().to_string())?;
        }
        std::fs::write(path, content)
            .change_context(FsError)
            .attach(path.display().to_string())
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), Report<FsError>> {
        std::fs::create_dir_all(path)
            .change_context(FsError)
            .attach(path.display().to_string())
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        std::fs::read_dir(path)
            .change_context(FsError)
            .attach(path.display().to_string())
            .map(|entries| {
                entries
                    .filter_map(std::result::Result::ok)
                    .map(|e| e.path())
                    .collect()
            })
    }

    fn list_dir_typed(&self, path: &Path) -> Result<Vec<DirEntry>, Report<FsError>> {
        std::fs::read_dir(path)
            .change_context(FsError)
            .attach(path.display().to_string())
            .map(|entries| {
                entries
                    .filter_map(std::result::Result::ok)
                    .map(|e| {
                        let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                        if is_dir {
                            DirEntry::dir(e.path())
                        } else {
                            DirEntry::file(e.path())
                        }
                    })
                    .collect()
            })
    }

    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        // Propagate root-level walk failures (missing/unreadable root); skip
        // individual entry errors so one unreadable file doesn't abort the walk.
        let mut it = walkdir::WalkDir::new(root).into_iter().peekable();
        // If the very first entry is an error, the root itself is inaccessible.
        if let Some(Err(_)) = it.peek() {
            it.next(); // consume the error
            return Err(Report::new(FsError)).attach(root.display().to_string());
        }
        Ok(it
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .map(walkdir::DirEntry::into_path)
            .collect())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn copy_file(&self, src: &Path, dst: &Path) -> Result<(), Report<FsError>> {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .change_context(FsError)
                .attach(parent.display().to_string())?;
        }
        std::fs::copy(src, dst)
            .change_context(FsError)
            .attach(src.display().to_string())
            .attach(dst.display().to_string())
            .map(|_| ())
    }

    fn name(&self) -> &'static str {
        "RealFs"
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeFs;

    // --- walk root-error propagation ---

    #[test]
    fn walk_returns_err_when_root_does_not_exist() {
        // Given a real filesystem.
        let fs = RealFs::new();

        // When walking a nonexistent root.
        let result = fs.walk(Path::new("/nonexistent-auditah-root-xyz"));

        // Then the walk returns an error.
        assert!(result.is_err());
    }

    // --- walk entry-error skipping (FakeFs models the same contract) ---

    #[test]
    fn walk_collects_only_files_excluding_directories() {
        // Given a fake filesystem with nested directories and files.
        let fs = FakeFs::with_files([("root/a.glb", "bytes"), ("root/sub/b.glb", "bytes")]);

        // When walking root recursively.
        let mut got = fs.walk(Path::new("root")).expect("walk readable root");

        // Then only the files are returned (directories excluded).
        got.sort();
        assert_eq!(
            got,
            vec![PathBuf::from("root/a.glb"), PathBuf::from("root/sub/b.glb"),]
        );
    }

    // --- create_dir_all ---

    #[test]
    fn create_dir_all_creates_directory_on_real_fs() {
        // Given a real filesystem and a temp dir.
        let fs = RealFs::new();
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("nested/LICENSES");

        // When creating the directory.
        fs.create_dir_all(&dir).expect("create_dir_all");

        // Then the directory exists on the real filesystem.
        assert!(dir.is_dir(), "{dir:?} should be a directory");
    }

    #[test]
    fn create_dir_all_makes_empty_dir_observable_as_exists_on_fake_fs() {
        // Given a fake filesystem with no files under LICENSES.
        let fs = FakeFs::default();

        // When explicitly creating the LICENSES directory.
        fs.create_dir_all(Path::new("/proj/LICENSES"))
            .expect("create_dir_all");

        // Then the empty dir is observable via exists(), and walk("/proj")
        // does NOT return the dir as if it were a file.
        assert!(fs.exists(Path::new("/proj/LICENSES")));
        assert!(fs.walk(Path::new("/proj")).expect("walk").is_empty());
    }

    // --- copy_file ---

    #[test]
    fn copy_file_preserves_binary_bytes_on_real_fs() {
        // Given a real filesystem with a binary asset (non-UTF-8 bytes).
        let fs = RealFs::new();
        let tmp = tempfile::tempdir().expect("tempdir");
        let bytes: &[u8] = &[0x00, 0xFF, 0xFE, 0x89, 0x50, 0x4E, 0x47];
        let src = tmp.path().join("asset.glb");
        std::fs::write(&src, bytes).expect("write src");
        let dst = tmp.path().join("nested/out/asset.glb");

        // When copying the asset to a nested destination.
        fs.copy_file(&src, &dst).expect("copy");

        // Then the destination is byte-identical to the source.
        assert_eq!(std::fs::read(&dst).expect("read dst"), bytes);
    }
}
