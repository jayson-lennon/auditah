//! `auditah license` — scaffold an attribution sidecar (file target) or a
//! directory `_manifest.toml` (directory target), provisioning the referenced
//! license into `LICENSES/` when it is well-known and absent.
//!
//! Dispatch is on the target's filesystem type: a file → sidecar, a directory →
//! manifest. There is no interactivity: `--id` and `--author` are required.
//! The project root is discovered by walking up from the target (or from the
//! target's parent for a file) for a `LICENSES/` directory; `--root` overrides
//! discovery by anchoring at the given path instead.

use std::path::{Path, PathBuf};

use crate::add::{write_manifest, write_sidecar};
use crate::add_license::{provision_license, year_from_clock};
use crate::discovery::resolver::MANIFEST_FILENAME;
use crate::model::attribution::AttributionRecord;
use crate::model::terms::Overrides;
use crate::services::Services;
use crate::AppError;
use clap::Args;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Scaffold an attribution sidecar (file target) or a directory manifest
/// (directory target).
///
/// `--id` and `--author` are required. The title defaults to the file stem
/// (file target) or the directory name (directory target); the year defaults to
/// the current clock year. `--modified` is only valid for a file target.
#[derive(Debug, Args)]
pub struct LicenseCmd {
    /// Target: a file (writes `<file>.attr.toml`) or a directory (writes
    /// `<dir>/_manifest.toml` covering the directory + its subdirs).
    pub target: PathBuf,

    /// SPDX/LicenseRef license id (e.g. CC0-1.0, CC-BY-4.0, MIT).
    #[arg(long)]
    pub id: String,

    /// Author / copyright holder. Required.
    #[arg(long)]
    pub author: String,

    /// Title. Defaults to the file stem (file) or directory name (directory).
    #[arg(long)]
    pub title: Option<String>,

    /// Copyright year. Defaults to the current clock year.
    #[arg(long)]
    pub year: Option<u16>,

    /// Source URL.
    #[arg(long)]
    pub source: Option<String>,

    /// Whether the asset has been modified. Only valid for a file target; a
    /// directory manifest cannot be "modified" and this flag errors if set.
    #[arg(long)]
    pub modified: bool,

    /// Override project-root discovery: walk up from `<root>` for `LICENSES/`
    /// instead of from the target. Relative values resolve against the cwd.
    #[arg(long)]
    pub root: Option<PathBuf>,
}

/// Run the `license` command.
///
/// `cwd` is the process working directory captured at program start, used to
/// anchor relative paths in `--root` or `target`.
///
/// # Errors
///
/// Returns an error if the target does not exist, `--modified` is set on a
/// directory target, no project root (ancestor `LICENSES/`) is found from the
/// resolved start, the requested license is unknown/custom and not already
/// present in `LICENSES/`, or the sidecar/manifest write fails.
pub fn run(cmd: &LicenseCmd, cwd: &Path) -> Result<CommandStatus, Report<AppError>> {
    let is_dir = std::fs::metadata(&cmd.target)
        .change_context(AppError)
        .attach("license target does not exist")
        .attach(cmd.target.display().to_string())?
        .is_dir();

    if is_dir && cmd.modified {
        return Err(Report::new(AppError).attach(
            "--modified is only valid for a file target; a directory manifest cannot be modified",
        ));
    }

    // Discover the project root: walk up from the target (directory branch) or
    // the target's parent (file branch), or from `--root` if given. `cwd`
    // anchors any relative start so discovery never reads the process env.
    let start = resolve_start(cmd, is_dir);
    let project_root = resolve_project_root(cwd, &start)?;
    let licenses_dir = project_root.join("LICENSES");
    let services = Services::real(&project_root)
        .change_context(AppError)
        .attach("failed to load services")?;

    provision_license(&services, &licenses_dir, &cmd.id)?;

    let record = build_record(cmd, &services, is_dir);
    if is_dir {
        write_manifest(&services, &cmd.target, &record).change_context(AppError)?;
        println!(
            "license: wrote {}/{MANIFEST_FILENAME}",
            cmd.target.display()
        );
    } else {
        write_sidecar(&services, &cmd.target, &record).change_context(AppError)?;
        println!("license: wrote {}.attr.toml", cmd.target.display());
    }
    Ok(CommandStatus::Success)
}

/// Pick the discovery start point: `--root` overrides; otherwise the target
/// itself (directory branch) or the target's parent (file branch).
fn resolve_start(cmd: &LicenseCmd, is_dir: bool) -> PathBuf {
    if let Some(root) = &cmd.root {
        return root.clone();
    }
    if is_dir {
        cmd.target.clone()
    } else {
        cmd.target
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    }
}

/// Resolve the project root for `start`, anchoring a relative `start` against
/// the injected `cwd` before walking up for a `LICENSES/` directory.
///
/// # Errors
///
/// Returns `AppError` when no `LICENSES/` is found walking up from `start`, or
/// `start` cannot be canonicalized.
fn resolve_project_root(cwd: &Path, start: &Path) -> Result<PathBuf, Report<AppError>> {
    crate::project::resolve_or_error(cwd, start)
}

/// Build the attribution record. `is_dir` selects the title default: directory
/// name for a directory target, file stem for a file target.
fn build_record(cmd: &LicenseCmd, services: &Services, is_dir: bool) -> AttributionRecord {
    let title = cmd
        .title
        .clone()
        .unwrap_or_else(|| default_title(&cmd.target, is_dir));
    AttributionRecord {
        title,
        author: cmd.author.clone(),
        year: cmd.year.unwrap_or_else(|| year_from_clock(&services.clock)),
        license: cmd.id.clone(),
        source: cmd.source.clone().unwrap_or_default(),
        modified: cmd.modified,
        package: None,
        overrides: Overrides::default(),
    }
}

/// Derive the title default from the target: directory name (directory target)
/// or file stem (file target). Falls back to an empty string if the path has no
/// usable name component.
fn default_title(target: &Path, is_dir: bool) -> String {
    let name = if is_dir {
        target.file_name()
    } else {
        target.file_stem()
    };
    name.and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // --- default_title ---

    #[test]
    fn default_title_uses_file_stem_for_file_target() {
        // Given a file target.
        let target = Path::new("/proj/sword.glb");

        // When deriving the default title for a file.
        let title = default_title(target, false);

        // Then the title is the file stem, not the full name.
        assert_eq!(title, "sword");
    }

    #[test]
    fn default_title_uses_directory_name_for_dir_target() {
        // Given a directory target.
        let target = Path::new("/proj/packs/animals");

        // When deriving the default title for a directory.
        let title = default_title(target, true);

        // Then the title is the directory name.
        assert_eq!(title, "animals");
    }

    // --- resolve_start ---

    fn cmd(target: &str, id: &str, author: &str) -> LicenseCmd {
        LicenseCmd {
            target: PathBuf::from(target),
            id: id.to_string(),
            author: author.to_string(),
            title: None,
            year: None,
            source: None,
            modified: false,
            root: None,
        }
    }

    #[test]
    fn resolve_start_uses_file_parent_when_no_root() {
        // Given a file target with no --root.
        let cmd = cmd("/proj/packs/sword.glb", "MIT", "Artist");

        // When picking the discovery start.
        let start = resolve_start(&cmd, false);

        // Then it starts at the file's parent directory.
        assert_eq!(start, PathBuf::from("/proj/packs"));
    }

    #[test]
    fn resolve_start_uses_target_itself_for_directory_when_no_root() {
        // Given a directory target with no --root.
        let cmd = cmd("/proj/packs/animals", "MIT", "Artist");

        // When picking the discovery start.
        let start = resolve_start(&cmd, true);

        // Then it starts at the target directory itself.
        assert_eq!(start, PathBuf::from("/proj/packs/animals"));
    }

    #[test]
    fn resolve_start_uses_root_override() {
        // Given a target and an explicit --root.
        let mut cmd = cmd("/proj/packs/sword.glb", "MIT", "Artist");
        cmd.root = Some(PathBuf::from("/elsewhere"));

        // When picking the discovery start.
        let start = resolve_start(&cmd, false);

        // Then it starts at the --root value, not the target's parent.
        assert_eq!(start, PathBuf::from("/elsewhere"));
    }

    // --- build_record ---

    #[test]
    fn build_record_defaults_title_to_file_stem_for_file_target() {
        // Given a file target with no explicit title.
        let cmd = cmd("/proj/sword.glb", "MIT", "Artist");
        let services = Services::real(Path::new(".")).unwrap();

        // When building the record for a file target.
        let record = build_record(&cmd, &services, false);

        // Then the title defaults to the file stem.
        assert_eq!(record.title, "sword");
    }

    #[test]
    fn build_record_defaults_source_to_empty_when_omitted() {
        // Given a command with no --source.
        let cmd = cmd("/proj/sword.glb", "MIT", "Artist");
        let services = Services::real(Path::new(".")).unwrap();

        // When building the record.
        let record = build_record(&cmd, &services, false);

        // Then source is the empty string.
        assert_eq!(record.source, "");
    }

    #[test]
    fn build_record_defaults_year_to_clock_year_when_omitted() {
        // Given a command with no --year.
        let cmd = cmd("/proj/sword.glb", "MIT", "Artist");
        let services = Services::real(Path::new(".")).unwrap();

        // When building the record.
        let record = build_record(&cmd, &services, false);

        // Then the year is the current clock year (>= 2025, sanity bound).
        assert!(
            record.year >= 2025,
            "year should be a plausible current year"
        );
    }
}
