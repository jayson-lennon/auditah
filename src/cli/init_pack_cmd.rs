//! `auditah init-pack` — write a directory `_manifest.toml`.

use std::path::PathBuf;

use crate::model::terms::Overrides;
use crate::AppError;
use clap::Args;

use crate::add::write_manifest;
use crate::discovery::resolver::MANIFEST_FILENAME;
use crate::model::attribution::AttributionRecord;
use crate::services::clock::ClockService;
use crate::services::Services;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Write a `_manifest.toml` covering a directory + its subdirs.
#[derive(Debug, Args)]
pub struct InitPackCmd {
    /// Directory to cover with a manifest.
    pub dir: PathBuf,

    /// SPDX license ID (e.g. CC0-1.0, CC-BY-3.0).
    #[arg(long)]
    pub license: String,

    /// Author / copyright holder.
    #[arg(long)]
    pub author: String,

    /// Copyright year.
    #[arg(long)]
    pub year: Option<u16>,

    /// Title prefix (defaults to the directory name).
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
/// Returns an error if services fail or the manifest write fails.
pub fn run(cmd: &InitPackCmd) -> Result<CommandStatus, Report<AppError>> {
    let title = cmd
        .title
        .clone()
        .or_else(|| {
            cmd.dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let services = Services::real(&cmd.dir).change_context(AppError)?;
    let record = {
        let year = cmd.year.unwrap_or_else(|| year_from_clock(&services.clock));
        AttributionRecord {
            title,
            author: cmd.author.clone(),
            year,
            license: cmd.license.clone(),
            source: cmd.source.clone().unwrap_or_default(),
            modified: false,
            package: None,
            overrides: Overrides::default(),
        }
    };
    write_manifest(&services, &cmd.dir, &record).change_context(AppError)?;
    println!("init-pack: wrote {}/{MANIFEST_FILENAME}", cmd.dir.display());
    Ok(CommandStatus::Success)
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
mod tests {
    use super::*;
    use crate::test_support::FakeClock;
    use std::sync::Arc;

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
}
