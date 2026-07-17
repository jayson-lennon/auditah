//! `auditah export` — copy a licensed asset (or a tree of them) from the source
//! project to a target location, carrying its attribution so the target is
//! audit-ready without the user hand-rebuilding sidecars or manifests.
//!
//! Flow:
//! 1. **Audit gate** — the *entire* source project must pass `auditah audit`
//!    first. Any compliance failure or technical error aborts before any byte
//!    is copied. Surfacing licensing failures early is the point: if the source
//!    isn't compliant, there may be no attribution to carry at all.
//! 2. **Ignored single-file guard** — if a single *file* source matches the
//!    merged exclude matcher, warn and copy nothing. The user named it
//!    explicitly, but honoring the matcher keeps junk out of the game dir.
//! 3. **Copy + carry attribution** — dispatch to [`crate::export::export_file`]
//!    or [`crate::export::export_dir`], which copy the asset(s) and synthesize
//!    the sidecar/manifest that must travel with it.
//!
//! The target project is never loaded, read, or modified. Export is strictly
//! asset + attribution transport; the user provisions licenses and config in
//! their own project via the other commands.

use std::path::PathBuf;

use clap::Args;
use error_stack::{Report, ResultExt};

use super::CommandStatus;
use crate::audit::build_excludes;
use crate::services::Services;
use crate::AppError;

/// Copy a licensed asset (file or directory) to a target location, carrying its
/// attribution.
#[derive(Debug, Args)]
pub struct ExportCmd {
    /// Source file or directory to export (within the source project).
    pub source: PathBuf,
    /// Target file or directory to write.
    pub target: PathBuf,
    /// Source project root (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Copy files even when the merged exclude matcher says to ignore them.
    #[arg(long = "copy-ignored")]
    pub copy_ignored: bool,
}

impl ExportCmd {
    /// Anchor all relative path arguments against `cwd` in place.
    ///
    /// `source`/`target` arrive raw from clap; this anchors them so command code
    /// operates on absolute paths regardless of the process cwd. `--root` is
    /// handled separately by the dispatch layer's root resolution.
    pub fn anchor_paths(&mut self, cwd: &std::path::Path) {
        self.source = crate::project::anchor(cwd, &self.source);
        self.target = crate::project::anchor(cwd, &self.target);
    }
}

/// Run the export command.
///
/// Returns `Ok(Success)` when the asset (or nothing, for an ignored
/// single-file source) was copied, and `Err(AppError)` on technical failures
/// or when the source project fails its audit gate.
///
/// # Errors
///
/// Returns an error if the audit gate fails (compliance or technical), if
/// exclude-matcher construction fails, or if the copy/synthesis fails.
pub fn run(services: &Services, cmd: &ExportCmd) -> Result<CommandStatus, Report<AppError>> {
    let report = crate::audit::run_audit(services)
        .change_context(AppError)
        .attach("export aborted: source project audit failed to run")?;
    if report.has_errors() {
        return Err(Report::new(AppError).attach(format!(
            "export aborted: source project has {} technical error(s) — fix before exporting",
            report.error_count()
        )));
    }
    if report.has_failures() {
        return Err(Report::new(AppError).attach(format!(
            "export aborted: source project has {} compliance failure(s) — the library must be fully licensed before exporting",
            report.fail_count()
        )));
    }

    // A single-file source matching the exclude matcher is warned-and-skipped.
    // `--copy-ignored` overrides: proceed to export_file, where the audit gate's
    // resolution still rejects an unattributable file (defense in depth).
    // Directory sources always recurse; the matcher is applied per-file inside.
    if !cmd.copy_ignored {
        if let Some(reason) = ignored_single_file_reason(services, &cmd.source) {
            eprintln!("export: skipping source file {reason}");
            return Ok(CommandStatus::Success);
        }
    }

    if cmd.source.is_dir() {
        crate::export::export_dir(services, &cmd.source, &cmd.target, cmd.copy_ignored)
            .change_context(AppError)
            .attach("failed to export directory")?;
    } else {
        crate::export::export_file(services, &cmd.source, &cmd.target, cmd.copy_ignored)
            .change_context(AppError)
            .attach("failed to export file")?;
    }

    Ok(CommandStatus::Success)
}

/// If `source` is a single file (not a dir) that the merged exclude matcher
/// excludes, return a human-readable reason. Otherwise return `None`.
fn ignored_single_file_reason(services: &Services, source: &std::path::Path) -> Option<String> {
    if source.is_dir() {
        return None;
    }
    let matcher = build_excludes(services).ok()?;
    let rel = source
        .strip_prefix(services.config.root())
        .unwrap_or(source);
    if matcher.is_excluded(rel) {
        Some(format!("{} (matches an exclude glob)", source.display()))
    } else {
        None
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::services::fs::FsService;
    use crate::services::Services;
    use crate::test_support::FakeFs;
    use std::path::Path;
    use std::sync::Arc;

    fn services_with(files: &[(&str, &str)]) -> Services {
        let fs = FsService::new(Arc::new(FakeFs::with_files(
            files.iter().map(|(p, c)| (*p, *c)),
        )));
        Services::test()
            .fs(fs)
            .config_root(Path::new("/proj"), Config::default())
            .build()
    }

    #[test]
    fn ignored_single_file_returns_reason_when_source_matches_exclude_glob() {
        // Given a source file that matches a DEFAULT_EXCLUDES glob (.git path).
        let services = services_with(&[("/proj/.git/config", "git")]);

        // When checking the ignored-single-file reason.
        let reason = ignored_single_file_reason(&services, Path::new("/proj/.git/config"));

        // Then a reason is returned describing the match.
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("exclude glob"));
    }

    #[test]
    fn ignored_single_file_returns_none_for_directory_source() {
        // Given a directory source path.
        let services = services_with(&[("/proj/pack", "")]);

        // When checking the ignored-single-file reason.
        let reason = ignored_single_file_reason(&services, Path::new("/proj/pack"));

        // Then no reason is returned (directories are never filtered here).
        assert!(reason.is_none());
    }

    #[test]
    fn ignored_single_file_returns_none_for_clean_asset() {
        // Given a regular asset file that no exclude glob matches.
        let services = services_with(&[("/proj/tree.glb", "tree")]);

        // When checking the ignored-single-file reason.
        let reason = ignored_single_file_reason(&services, Path::new("/proj/tree.glb"));

        // Then no reason is returned.
        assert!(reason.is_none());
    }

    #[test]
    fn anchor_paths_joins_relative_source_and_target_against_cwd() {
        // Given an export command with relative source/target.
        let mut cmd = ExportCmd {
            source: PathBuf::from("lib/sword.glb"),
            target: PathBuf::from("game/sword.glb"),
            root: PathBuf::from("."),
            copy_ignored: false,
        };

        // When anchoring against a cwd.
        cmd.anchor_paths(Path::new("/work"));

        // Then both paths are joined against cwd.
        assert_eq!(cmd.source, PathBuf::from("/work/lib/sword.glb"));
        assert_eq!(cmd.target, PathBuf::from("/work/game/sword.glb"));
    }
}
