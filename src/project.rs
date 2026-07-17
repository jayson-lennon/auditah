//! Project-root discovery: locate the project that owns a given path by
//! walking up the directory tree looking for a `LICENSES/` directory marker.
//!
//! The marker is `LICENSES/` only — not `auditah.toml`. A directory `d` is the
//! project root when `d/LICENSES` exists. The function returns `d` itself (the
//! directory containing `LICENSES/`), never `LICENSES/`'s parent's parent.
//!
//! Used by the LICENSES-dependent commands (`audit`, `generate`, `license provision`,
//! `init-pack`) so a subdirectory invocation resolves the real project. The
//! unbounded walk ends naturally when [`Path::parent`] returns `None` at the
//! filesystem root.

use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};

use crate::services::FsService;

/// Walk up from `start` to the filesystem root, returning the first directory
/// whose child `LICENSES/` exists — that directory **is** the project root.
/// Returns `None` if no `LICENSES/` is found on the ancestor chain.
///
/// The search begins at `start` itself: if `start/LICENSES` exists, `start` is
/// returned.
#[must_use]
pub fn find_project_root(fs: &FsService, start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if fs.exists(&d.join("LICENSES")) {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Build a real-fs `FsService` for the pre-services discovery probe.
///
/// Discovery runs before [`crate::services::Services`] exist, so it needs its
/// own throwaway backend.
fn real_fs() -> FsService {
    use crate::services::RealFs;
    use std::sync::Arc;
    FsService::new(Arc::new(RealFs::new()))
}

/// Resolve the project root for `start` (the `--root` flag value),
/// hard-erroring when no ancestor `LICENSES/` directory exists.
///
/// `cwd` is the process working directory captured at program start and
/// injected by the caller; it is used to anchor a *relative* `start` (e.g.
/// `.`) before canonicalizing, so the canonicalize step resolves symlinks
/// only and never reads process cwd. Used by the LICENSES-dependent commands
/// (`audit`, `generate`, `license provision`, `init-pack`) as their first step. The
/// error message points the user at `auditah init`, since `init` is the sole
/// creator of `LICENSES/`.
///
/// # Errors
///
/// Returns `AppError` (no fallback to `start`) when no `LICENSES/` is found
/// walking up from `start` to the filesystem root.
pub fn resolve_or_error(cwd: &Path, start: &Path) -> Result<PathBuf, Report<crate::AppError>> {
    // Anchor a relative `start` (e.g. `.`) against the injected `cwd` so that
    // `canonicalize` operates on an absolute path and resolves symlinks only,
    // never the process cwd. `find_project_root` itself stays backend-agnostic
    // (FakeFs tests use virtual absolute paths), so the real-fs canonicalize
    // lives here in the entry point.
    let absolute = if start.is_relative() {
        cwd.join(start)
    } else {
        start.to_path_buf()
    };
    let start = std::fs::canonicalize(&absolute)
        .change_context(crate::AppError)
        .attach(format!(
            "failed to canonicalize root {}",
            absolute.display()
        ))?;
    find_project_root(&real_fs(), &start).ok_or_else(|| {
        Report::new(crate::AppError)
            .attach(format!(
                "no LICENSES/ directory found walking up from {}",
                start.display()
            ))
            .attach("run `auditah init` first, or run from inside a project")
    })
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeFs;
    use std::sync::Arc;

    fn fake_fs() -> FsService {
        FsService::new(Arc::new(FakeFs::default()))
    }

    #[test]
    fn find_project_root_resolves_ancestor() {
        // Given a fs with LICENSES/ at /proj and a deeper start path.
        let fs = fake_fs();
        fs.write(Path::new("/proj/LICENSES/.keep"), "").unwrap();

        // When walking up from a nested directory.
        let found = find_project_root(&fs, Path::new("/proj/sub/deep"));

        // Then it returns the project root /proj (the dir containing LICENSES/).
        assert_eq!(found.as_deref(), Some(Path::new("/proj")));
    }

    #[test]
    fn find_project_root_returns_none_when_absent() {
        // Given a fs with no LICENSES/ anywhere.
        let fs = fake_fs();

        // When walking up from an arbitrary path.
        let found = find_project_root(&fs, Path::new("/nowhere/sub"));

        // Then nothing is found.
        assert!(found.is_none());
    }

    #[test]
    fn find_project_root_search_begins_at_start() {
        // Given a fs with LICENSES/ at /proj.
        let fs = fake_fs();
        fs.write(Path::new("/proj/LICENSES/.keep"), "").unwrap();

        // When walking up from /proj itself (the root).
        let found = find_project_root(&fs, Path::new("/proj"));

        // Then the search begins at the start dir and returns it immediately.
        assert_eq!(found.as_deref(), Some(Path::new("/proj")));
    }
    #[test]
    fn resolve_or_error_anchors_relative_start_against_injected_cwd() {
        // Given a real tempdir with LICENSES/ at its root and a subdir below it.
        // resolve_or_error uses std::fs::canonicalize, so this test needs a real
        // filesystem (FakeFs cannot model canonicalize).
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("LICENSES")).unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();

        // When resolving a RELATIVE start (".") anchored at the injected cwd,
        // which points at the subdir (whose ancestor holds LICENSES/).
        let cwd = root.join("sub");
        let resolved = resolve_or_error(&cwd, Path::new(".")).expect("resolve");

        // Then it finds the ancestor LICENSES/ root, without reading process cwd.
        assert_eq!(resolved, std::fs::canonicalize(root).unwrap());
    }
}
