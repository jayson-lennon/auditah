//! Shared helpers for integration tests. Reduces duplication across the
//! `tests/*.rs` pipeline files. Each file pulls these in via `mod common;`.
//!
//! `#![allow(dead_code)]` is intentional: each integration test crate compiles
//! this module independently, so helpers used only by *other* test files
//! would otherwise appear dead.

#![allow(dead_code)]

use std::sync::Arc;

use auditah::audit::report::{AuditReport, FindingCode};
use auditah::config::Config;
use auditah::model::attribution::AttributionRecord;
use auditah::model::terms::Overrides;
use auditah::registry::LicenseRegistry;
use auditah::services::fs::{FsService, RealFs};
use auditah::services::Services;

/// Build a real-filesystem [`Services`] with the embedded license registry
/// (no project-local licenses). Used by audit/credits/add pipeline tests.
#[must_use]
pub fn services() -> Services {
    Services {
        fs: FsService::new(Arc::new(RealFs::new())),
        registry: LicenseRegistry::embedded_only(),
    }
}

/// Seed `LICENSES/<id>.txt` for every embedded license so audit's
/// `MissingLicenseText` check passes in pass-clean scenarios.
pub fn seed_licenses(root: &std::path::Path) {
    let reg = LicenseRegistry::embedded_only();
    let dir = root.join("LICENSES");
    std::fs::create_dir_all(&dir).expect("create LICENSES dir");
    for entry in reg.entries() {
        std::fs::write(dir.join(format!("{}.txt", entry.id)), &entry.text)
            .expect("write LICENSES file");
    }
}

/// Non-commercial project config with no exclude globs.
#[must_use]
pub fn non_commercial_config() -> Config {
    Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    }
}

/// Alias for [`non_commercial_config`] (shorter name used by some tests).
#[must_use]
pub fn cfg() -> Config {
    non_commercial_config()
}

/// Alias for [`non_commercial_config`] (matches `auditah.toml` config naming).
#[must_use]
pub fn config() -> Config {
    non_commercial_config()
}

/// Commercial project config with no exclude globs.
#[must_use]
pub fn commercial_config() -> Config {
    Config {
        commercial_project: true,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    }
}

/// Collect the finding codes for assets whose name contains `needle`.
#[must_use]
pub fn codes_for(report: &AuditReport, needle: &str) -> Vec<FindingCode> {
    report
        .findings
        .iter()
        .filter(|f| f.asset.to_string_lossy().contains(needle))
        .map(|f| f.code)
        .collect()
}

/// Build a minimal valid attribution record under `license`.
#[must_use]
pub fn record(license: &str) -> AttributionRecord {
    AttributionRecord {
        title: "Sample".to_string(),
        author: "Artist".to_string(),
        year: 2020,
        license: license.to_string(),
        source: "https://example.com".to_string(),
        modified: false,
        package: None,
        overrides: Overrides::default(),
    }
}

use auditah::discovery::enumerator::ExcludeMatcher;

/// Build a real-filesystem `FsService` (no registry). Used by discovery tests
/// that exercise walkdir/globset against a real temp fs.
#[must_use]
pub fn real_fs() -> FsService {
    FsService::new(Arc::new(RealFs::new()))
}

/// Default exclude matcher (built-in excludes, no user globs).
#[must_use]
pub fn default_excludes() -> ExcludeMatcher {
    ExcludeMatcher::new(&auditah::discovery::all_excludes(&[]))
        .expect("default exclude globs are valid")
}
