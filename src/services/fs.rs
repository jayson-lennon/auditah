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

    /// List immediate children (files + dirs) of `path`.
    ///
    /// # Errors
    /// Returns [`FsError`] if the directory cannot be read.
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;

    /// Recursively walk `root`, returning every file path beneath it.
    ///
    /// # Errors
    /// Returns [`FsError`] if the walk fails.
    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;

    /// Whether `path` exists on the backing filesystem.
    fn exists(&self, path: &Path) -> bool;

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

    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        // Individual entry read errors are skipped; a walk over readable
        // entries is infallible from here.
        Ok(walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .collect())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn name(&self) -> &'static str {
        "RealFs"
    }
}
