//! Shared test infrastructure for auditah.
//!
//! `FakeFs` is the single in-memory `FsBackend` used by every unit and
//! integration test. It replaces the seven divergent, behaviour-conflicting
//! copies that previously lived in each module's `#[cfg(test)]` block.
//!
//! Semantics mirror `RealFs` faithfully so tests exercise real discovery:
//!   - `list_dir(p)` → immediate children of `p` (keys whose parent is `p`).
//!   - `walk(root)`  → every file recursively under `root` (keys starting with `root`).
//!
//! IO-error injection: register a path (or walk-root) on the relevant
//! `fail_*` set; the matching operation returns `Err(FsError)` before any
//! lookup. This lets error-scenario tests trigger technical failures without
//! a real filesystem.
//!
//! Gated behind the `test-helper` Cargo feature and `#[doc(hidden)]` so it
//! ships in no default build and never appears in public docs.

#![cfg(feature = "test-helper")]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use error_stack::Report;
use parking_lot::Mutex;

use crate::services::fs::{FsBackend, FsError};

/// Internal mutable state behind a single lock.
#[derive(Debug, Default)]
struct FsState {
    files: HashMap<PathBuf, String>,
    fail_reads: HashSet<PathBuf>,
    fail_writes: HashSet<PathBuf>,
    fail_walks: HashSet<PathBuf>,
    fail_list_dirs: HashSet<PathBuf>,
}

impl FsState {
    fn empty() -> Self {
        Self {
            files: HashMap::new(),
            fail_reads: HashSet::new(),
            fail_writes: HashSet::new(),
            fail_walks: HashSet::new(),
            fail_list_dirs: HashSet::new(),
        }
    }
}

/// In-memory `FsBackend` for tests. Construct via [`FakeFs::with_files`] then
/// chain builder methods (`insert`, `fail_read`, `fail_write`, `fail_walk`,
/// `fail_list_dir`); wrap in `FsService::new(Arc::new(...))` when done.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct FakeFs {
    state: Mutex<FsState>,
}

impl FakeFs {
    /// Create a fake pre-populated with `files`. Each entry is `(path, content)`.
    #[must_use]
    pub fn with_files<I, P, S>(files: I) -> Self
    where
        I: IntoIterator<Item = (P, S)>,
        P: Into<PathBuf>,
        S: Into<String>,
    {
        let mut state = FsState::empty();
        for (path, content) in files {
            state.files.insert(path.into(), content.into());
        }
        Self {
            state: Mutex::new(state),
        }
    }

    /// Insert a file at `path` with `content`. Consuming builder.
    #[must_use]
    pub fn insert<P, S>(self, path: P, content: S) -> Self
    where
        P: Into<PathBuf>,
        S: Into<String>,
    {
        self.state.lock().files.insert(path.into(), content.into());
        self
    }

    /// Force `read_to_string(path)` to return `Err(FsError)`.
    #[must_use]
    pub fn fail_read<P>(self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.state.lock().fail_reads.insert(path.into());
        self
    }

    /// Force `write(path, _)` to return `Err(FsError)`.
    #[must_use]
    pub fn fail_write<P>(self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.state.lock().fail_writes.insert(path.into());
        self
    }

    /// Force `walk(root)` to return `Err(FsError)`.
    #[must_use]
    pub fn fail_walk<P>(self, root: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.state.lock().fail_walks.insert(root.into());
        self
    }

    /// Force `list_dir(path)` to return `Err(FsError)`.
    #[must_use]
    pub fn fail_list_dir<P>(self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.state.lock().fail_list_dirs.insert(path.into());
        self
    }
}

impl FsBackend for FakeFs {
    fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>> {
        let state = self.state.lock();
        if state.fail_reads.contains(path) {
            return Err(Report::new(FsError));
        }
        state
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| Report::new(FsError))
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>> {
        let mut state = self.state.lock();
        if state.fail_writes.contains(path) {
            return Err(Report::new(FsError));
        }
        state.files.insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        let state = self.state.lock();
        if state.fail_list_dirs.contains(path) {
            return Err(Report::new(FsError));
        }
        Ok(state
            .files
            .keys()
            .filter(|k| k.parent() == Some(path))
            .cloned()
            .collect())
    }

    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        let state = self.state.lock();
        if state.fail_walks.contains(root) {
            return Err(Report::new(FsError));
        }
        Ok(state
            .files
            .keys()
            .filter(|k| k.starts_with(root))
            .cloned()
            .collect())
    }

    fn exists(&self, path: &Path) -> bool {
        let state = self.state.lock();
        // A path exists if it's a seeded file, or if any seeded file lives
        // beneath it (implicit directory).
        state.files.contains_key(path) || state.files.keys().any(|k| k.starts_with(path))
    }

    fn name(&self) -> &'static str {
        "FakeFs"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Given a fake seeded with a nested file tree.
    // When listing a directory.
    // Then only immediate children are returned.
    #[test]
    fn list_dir_returns_immediate_children_only() {
        let fs = FakeFs::with_files([
            (Path::new("/proj/a.txt"), "1"),
            (Path::new("/proj/b.txt"), "2"),
            (Path::new("/proj/sub/c.txt"), "3"),
            (Path::new("/proj/sub/deep/d.txt"), "4"),
        ]);
        let children = fs.list_dir(Path::new("/proj")).unwrap();
        let names: Vec<String> = children
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"b.txt".to_string()));
        // sub/ contents are not immediate children of /proj.
        assert!(
            !children
                .iter()
                .any(|p| p.to_string_lossy().contains("c.txt")),
            "nested files must not appear in parent list_dir"
        );
    }

    // Given a fake seeded with a nested file tree.
    // When walking the root.
    // Then every file recursively under the root is returned.
    #[test]
    fn walk_returns_all_files_recursively() {
        let fs = FakeFs::with_files([
            (Path::new("/proj/a.txt"), "1"),
            (Path::new("/proj/sub/b.txt"), "2"),
            (Path::new("/proj/sub/deep/c.txt"), "3"),
            (Path::new("/other/x.txt"), "4"),
        ]);
        let files = fs.walk(Path::new("/proj")).unwrap();
        assert_eq!(files.len(), 3, "only files under /proj");
        assert!(!files.iter().any(|p| p.to_string_lossy().contains("/other/")));
    }

    // Given a fake with a registered read failure.
    // When reading that path.
    // Then the operation errors.
    #[test]
    fn fail_read_injects_error() {
        let fs = FakeFs::with_files([(Path::new("/x"), "data")]).fail_read(Path::new("/x"));
        assert!(fs.read_to_string(Path::new("/x")).is_err());
    }

    // Given a fake with a registered write failure.
    // When writing that path.
    // Then the operation errors and no file is stored.
    #[test]
    fn fail_write_injects_error_and_does_not_store() {
        let fs = FakeFs::default().fail_write(Path::new("/out"));
        assert!(fs.write(Path::new("/out"), "x").is_err());
        assert!(!fs.exists(Path::new("/out")));
    }

    // Given a fake with a registered walk failure on a root.
    // When walking that root.
    // Then the operation errors.
    #[test]
    fn fail_walk_injects_error() {
        let fs = FakeFs::default().fail_walk(Path::new("/proj"));
        assert!(fs.walk(Path::new("/proj")).is_err());
    }

    // Given a fake with a registered list_dir failure.
    // When listing that directory.
    // Then the operation errors.
    #[test]
    fn fail_list_dir_injects_error() {
        let fs = FakeFs::default().fail_list_dir(Path::new("/proj"));
        assert!(fs.list_dir(Path::new("/proj")).is_err());
    }
    // Given a fake with a file nested under a directory path.
    // When checking whether the directory path exists.
    // Then it returns true (implicit directory).
    #[test]
    fn exists_returns_true_for_implicit_directory() {
        let fs = FakeFs::with_files([(Path::new("/proj/sub/a.txt"), "x")]);
        assert!(fs.exists(Path::new("/proj")));
        assert!(fs.exists(Path::new("/proj/sub")));
    }

}
