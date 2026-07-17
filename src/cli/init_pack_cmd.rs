//! `auditah init-pack` — write a directory `_manifest.toml`, provisioning the
//! referenced license into `LICENSES/` when it is well-known and absent.

use std::path::{Path, PathBuf};

use crate::model::terms::Overrides;
use crate::AppError;
use clap::Args;

use crate::add::write_manifest;
use crate::discovery::resolver::MANIFEST_FILENAME;
use crate::model::attribution::AttributionRecord;
use crate::services::clock::ClockService;
use crate::services::{FsService, Services};
use crate::well_known::{self, ResolveResult};
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Write a `_manifest.toml` covering the current directory + its subdirs.
///
/// Provisioning: if the requested license is absent from the discovered
/// `LICENSES/` and is a well-known SPDX id, it is installed there (text + grid).
/// Unknown/custom ids that are not already present are a hard error — the user
/// must run `auditah add-license --custom` first. There is no `--dir`/`--root`
/// flag: `init-pack` always runs from the cwd and discovers `LICENSES/` by
/// walking up the tree.
#[derive(Debug, Args)]
pub struct InitPackCmd {
    /// SPDX license ID (e.g. CC0-1.0, CC-BY-4.0, MIT).
    #[arg(long)]
    pub license: String,

    /// Author / copyright holder.
    #[arg(long)]
    pub author: String,

    /// Copyright year.
    #[arg(long)]
    pub year: Option<u16>,

    /// Title (defaults to the current directory name).
    #[arg(long)]
    pub title: Option<String>,

    /// Source URL.
    #[arg(long)]
    pub source: Option<String>,
}

/// Run the init-pack command.
///
/// # Errors
///
/// Returns an error if the cwd cannot be determined, no `LICENSES/` is found
/// walking up from the cwd, the requested license is unknown/custom and not
/// already present in `LICENSES/`, or the manifest/provisioning writes fail.
pub fn run(cmd: &InitPackCmd) -> Result<CommandStatus, Report<AppError>> {
    let cwd = std::env::current_dir().change_context(AppError)?;

    // Discover the project's LICENSES/ by walking up from the cwd.
    let licenses_dir = find_licenses_dir(&real_fs(), &cwd).ok_or_else(|| {
        Report::new(AppError)
            .attach("no LICENSES/ directory found walking up from the cwd")
            .attach(cwd.display().to_string())
            .attach("run from inside a project, or create LICENSES/ first")
    })?;

    // Services::real loads the registry from <root>/LICENSES, so pass the
    // project root (the parent of LICENSES/), not the LICENSES/ dir itself.
    let project_root = licenses_dir.parent().unwrap_or(Path::new("."));
    let services = Services::real(project_root)
        .change_context(AppError)
        .attach("failed to load services")?;

    provision_license(&services, &licenses_dir, &cmd.license)?;
    write_manifest_record(cmd, &services, &cwd)?;

    println!("init-pack: wrote {}/{MANIFEST_FILENAME}", cwd.display());
    Ok(CommandStatus::Success)
}

/// Provision the requested license into `licenses_dir` if it is absent.
///
/// Matrix: already-present -> skip; well-known -> install text + grid;
/// unknown/custom -> hard error pointing at `add-license --custom`.
///
/// # Errors
///
/// Returns an error if the license is not present and not a known SPDX id, or
/// if the text/grid writes fail.
fn provision_license(
    services: &Services,
    licenses_dir: &Path,
    license_id: &str,
) -> Result<(), Report<AppError>> {
    // write_grid/license_grid_path take the PROJECT ROOT (they join "LICENSES").
    let project_root = licenses_dir.parent().unwrap_or(Path::new("."));
    let grid_path = crate::add_license::license_grid_path(project_root, license_id);
    if grid_path.exists() {
        return Ok(());
    }

    match well_known::resolve(license_id) {
        ResolveResult::NotFound => Err(Report::new(AppError)
            .attach(format!(
                "license {license_id:?} is not in LICENSES/ and is not a known SPDX id"
            ))
            .attach("run `auditah add-license --custom <name>` to create it first")),
        ResolveResult::Found(canonical) => {
            let text = well_known::extract_text(&canonical);
            crate::add_license::write_text(services, project_root, &canonical, &text)
                .change_context(AppError)?;
            let grid = well_known::extract_grid(&canonical)
                .unwrap_or_else(|| crate::add_license::render_license_template(&canonical));
            crate::add_license::write_grid(services, project_root, &canonical, &grid)
                .change_context(AppError)?;
            eprintln!("init-pack: provisioned {canonical} into LICENSES/");
            Ok(())
        }
    }
}

/// Build the record (title defaults to the cwd name) and write the manifest.
fn write_manifest_record(
    cmd: &InitPackCmd,
    services: &Services,
    cwd: &Path,
) -> Result<(), Report<AppError>> {
    let title = cmd
        .title
        .clone()
        .or_else(|| cwd.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let record = AttributionRecord {
        title,
        author: cmd.author.clone(),
        year: cmd.year.unwrap_or_else(|| year_from_clock(&services.clock)),
        license: cmd.license.clone(),
        source: cmd.source.clone().unwrap_or_default(),
        modified: false,
        package: None,
        overrides: Overrides::default(),
    };
    write_manifest(services, cwd, &record).change_context(AppError)
}

/// Build a real-fs `FsService` just for discovery (before services exist).
fn real_fs() -> FsService {
    use crate::services::RealFs;
    use std::sync::Arc;
    FsService::new(Arc::new(RealFs::new()))
}

/// Walk up from `start` to the filesystem root, returning the first ancestor
/// `LICENSES/` directory that exists, or `None` if none is found.
///
/// Unlike [`crate::discovery::resolver`]'s manifest walker, this has no upper
/// bound: `init-pack` runs from the cwd and must find the project's `LICENSES/`
/// wherever it lives up the tree. The loop ends naturally when `parent()`
/// returns `None` at the filesystem root.
fn find_licenses_dir(fs: &FsService, start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join("LICENSES");
        if fs.exists(&candidate) {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Resolve the copyright year when `--year` is omitted: read the wall
/// clock via `clock` and map epoch seconds to a calendar year. On a broken
/// or pre-epoch clock, fall back to `2026`.
fn year_from_clock(clock: &ClockService) -> u16 {
    clock.now_epoch_secs().map_or(2026, year_from_epoch_secs)
}

/// Map Unix epoch seconds to an approximate calendar year.
///
/// Uses a Julian year (`31_557_600` seconds); year-boundary drift of ±1 day
/// is acceptable for copyright-year attribution.
fn year_from_epoch_secs(secs: u64) -> u16 {
    u16::try_from(secs / 31_557_600 + 1970).unwrap_or(2026)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::add_license::{license_grid_path, license_text_path};
    use crate::test_support::{FakeClock, FakeFs};
    use std::sync::Arc;
    use temptree::temptree;

    fn fake_fs() -> FsService {
        FsService::new(Arc::new(FakeFs::default()))
    }

    #[test]
    fn year_from_epoch_secs_maps_a_known_past_year() {
        // Given epoch seconds for 2019-01-01.
        let secs = 1_546_322_400;

        // When mapping to a year.
        let year = year_from_epoch_secs(secs);

        // Then the year is 2019.
        assert_eq!(year, 2019);
    }

    #[test]
    fn year_from_epoch_secs_maps_a_current_era_year() {
        // Given epoch seconds for 2025-01-01.
        let secs = 1_735_689_600;

        // When mapping to a year.
        let year = year_from_epoch_secs(secs);

        // Then the year is 2025.
        assert_eq!(year, 2025);
    }

    #[test]
    fn year_from_epoch_secs_maps_epoch_zero_to_1970() {
        // Given the epoch itself.
        let secs = 0;

        // When mapping to a year.
        let year = year_from_epoch_secs(secs);

        // Then the year is 1970, not 0 (the original off-by-1970 bug).
        assert_eq!(year, 1970);
    }

    #[test]
    fn year_from_epoch_secs_falls_back_when_future_overflows_u16() {
        // Given an absurd future timestamp that would overflow u16.
        let secs = u64::MAX;

        // When mapping to a year.
        let year = year_from_epoch_secs(secs);

        // Then the year is the 2026 fallback rather than panicking.
        assert_eq!(year, 2026);
    }

    #[test]
    fn year_from_clock_maps_a_fixed_instant_to_its_year() {
        // Given a FakeClock pinned to a 2019-01-01 epoch-second instant.
        let clock = ClockService::new(Arc::new(FakeClock::fixed(1_546_322_400)));

        // When resolving the default year from the clock.
        let year = year_from_clock(&clock);

        // Then the year is 2019 (not 56 / 0 — the original bug).
        assert_eq!(year, 2019);
    }

    #[test]
    fn year_from_clock_falls_back_to_2026_when_clock_is_broken() {
        // Given a FakeClock that always fails (models a pre-epoch clock).
        let clock = ClockService::new(Arc::new(FakeClock::broken()));

        // When resolving the default year from the broken clock.
        let year = year_from_clock(&clock);

        // Then the year is the 2026 fallback rather than erroring or panicking.
        assert_eq!(year, 2026);
    }

    // --- find_licenses_dir discovery ---

    #[test]
    fn find_licenses_dir_resolves_ancestor() {
        // Given a fs with LICENSES/ at /proj and a deeper start path.
        let fs = fake_fs();
        fs.write(Path::new("/proj/LICENSES/.keep"), "").unwrap();

        // When walking up from a nested directory.
        let found = find_licenses_dir(&fs, Path::new("/proj/sub/deep"));

        // Then it returns the ancestor LICENSES/.
        assert_eq!(found.as_deref(), Some(Path::new("/proj/LICENSES")));
    }

    #[test]
    fn find_licenses_dir_returns_none_when_absent() {
        // Given a fs with no LICENSES/ anywhere.
        let fs = fake_fs();

        // When walking up from an arbitrary path.
        let found = find_licenses_dir(&fs, Path::new("/nowhere/sub"));

        // Then nothing is found.
        assert!(found.is_none());
    }

    // --- Provisioning matrix (provision_license) ---
    //
    // These exercise the four-cell matrix directly: the cwd-coupled run()
    // dispatches to provision_license after discovery, so the matrix is the
    // observable behavior under test. Each test pins one cell.

    fn empty_project_with_licenses() -> (tempfile::TempDir, std::path::PathBuf) {
        let tree = temptree! { LICENSES: {} };
        let root = tree.path().to_path_buf();
        (tree, root)
    }

    #[test]
    fn provision_skips_when_license_grid_already_present() {
        // Given a project whose LICENSES/ already has MIT.toml.
        let (_tree, root) = empty_project_with_licenses();
        std::fs::write(
            root.join("LICENSES/MIT.toml"),
            "id = \"MIT\"\nname = \"handwritten\"\nurl = \"https://x\"\n[terms]\nrequires_attribution = false\nrequires_license_notice = true\nrequires_source_disclosure = false\nderivatives = \"allowed\"\nrequires_modification_notice = false\nallows_commercial_use = true\nallows_redistribution = true\nmanual_review = false\n",
        )
        .unwrap();
        let svc = Services::real(&root).unwrap();
        let original = std::fs::read_to_string(root.join("LICENSES/MIT.toml")).unwrap();

        // When provisioning MIT.
        provision_license(&svc, &root.join("LICENSES"), "MIT").unwrap();

        // Then the grid is untouched (not overwritten) and no .txt appeared.
        let after = std::fs::read_to_string(root.join("LICENSES/MIT.toml")).unwrap();
        assert_eq!(original, after, "existing grid must not be overwritten");
        assert!(!root.join("LICENSES/MIT.txt").exists());
    }

    #[test]
    fn provision_installs_text_and_grid_for_well_known_id() {
        // Given a project whose LICENSES/ is empty.
        let (_tree, root) = empty_project_with_licenses();
        let svc = Services::real(&root).unwrap();

        // When provisioning MIT (well-known, absent).
        provision_license(&svc, &root.join("LICENSES"), "MIT").unwrap();

        // Then both MIT.txt and MIT.toml are written to the discovered LICENSES/.
        assert!(
            license_text_path(&root, "MIT").exists(),
            "MIT.txt must be written"
        );
        assert!(
            license_grid_path(&root, "MIT").exists(),
            "MIT.toml must be written"
        );
        let grid = std::fs::read_to_string(license_grid_path(&root, "MIT")).unwrap();
        assert!(
            grid.contains("id = \"MIT\""),
            "grid must carry the canonical id"
        );
    }

    #[test]
    fn provision_hard_errors_when_id_is_unknown_and_absent() {
        // Given a project whose LICENSES/ has no StudioEULA grid.
        let (_tree, root) = empty_project_with_licenses();
        let svc = Services::real(&root).unwrap();

        // When provisioning an unknown id.
        let result = provision_license(&svc, &root.join("LICENSES"), "StudioEULA");

        // Then it hard-errors with a pointer at `add-license --custom`.
        let report = result.expect_err("unknown id must error");
        let rendered = format!("{report:?}");
        assert!(
            rendered.contains("--custom"),
            "error must mention add-license --custom: {rendered}"
        );
        assert!(
            rendered.contains("StudioEULA"),
            "error must name the offending id: {rendered}"
        );
    }
}
