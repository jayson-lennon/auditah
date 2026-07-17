//! Shared helpers for integration tests. Reduces duplication across the
//! `tests/*.rs` pipeline files. Each file pulls these in via `mod common;`.
//!
//! `#![allow(dead_code)]` is intentional: each integration test crate compiles
//! this module independently, so helpers used only by *other* test files
//! would otherwise appear dead.
#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use auditah::audit::report::{AuditReport, FindingCode};
use auditah::config::Config;
use auditah::model::attribution::AttributionRecord;
use auditah::model::terms::{Derivatives, LicenseTerms, Overrides};
use auditah::registry::{LicenseRegistry, LicenseRegistryService, LicenseSpec};
use auditah::services::clock::RealClock;
use auditah::services::config::ConfigService;
use auditah::services::fs::{FsService, RealFs};
use auditah::services::{ClockService, Services};
use auditah::test_support::ServicesTestBuilder;

/// Build a real-filesystem [`Services`] with the given registry.
///
/// Each test declares the licenses it expects to fulfill it via the
/// [`LicenseRegistryBuilder`] (see [`services_with`] / [`services_empty`]).
fn services_from(registry: LicenseRegistry, root: &Path, config: Config) -> Services {
    Services::test()
        .fs(FsService::new(Arc::new(RealFs::new())))
        .registry(LicenseRegistryService::new(Arc::new(registry)))
        .clock(ClockService::new(Arc::new(RealClock::new())))
        .config(ConfigService::new(Arc::from(root), Arc::new(config)))
        .build()
}

/// Build [`Services`] with a registry constructed from the given specs.
///
/// Tests that need a resolvable license add it here:
/// `services_with(&root, cfg(), [LicenseSpec::new("LicenseRef-Asset").text("...")])`.
#[must_use]
pub fn services_with(
    root: &Path,
    config: Config,
    specs: impl IntoIterator<Item = LicenseSpec>,
) -> Services {
    let mut builder = LicenseRegistry::builder();
    for spec in specs {
        builder = builder.license(spec);
    }
    services_from(builder.build(), root, config)
}

/// Build [`Services`] with an empty registry (no licenses resolvable).
///
/// Audit tests that exercise `UnknownLicense` start here.
#[must_use]
pub fn services_empty(root: &Path, config: Config) -> Services {
    services_from(LicenseRegistry::builder().build(), root, config)
}

/// Build a fully-real [`Services`] for a project root: loads `auditah.toml`
/// and `LICENSES/` from disk the same way `main` does. Used by CLI tests that
/// exercise `*_cmd::run(&services, &cmd)`.
#[must_use]
pub fn real_services(root: &Path) -> Services {
    ServicesTestBuilder::load_from_disk(root)
        .expect("load real services from disk")
        .build()
}

/// Resolve an ancestor `LICENSES/` from `start` (anchored at `cwd`) and build
/// a fully-real [`Services`] from the resolved root — the same wiring `main`'s
/// `dispatch` does for LICENSES-discovering commands (audit/generate/license provision).
#[must_use]
pub fn resolve_services(cwd: &Path, start: &Path) -> Services {
    let root = auditah::project::resolve_or_error(cwd, start).expect("resolve root");
    real_services(&root)
}

/// Write `LICENSES/<id>.txt` for each given id under `root`.
///
/// The registry builder handles the `.toml` grids; this handles the legal
/// text files that audit's `MissingLicenseText` check gates on. Call after
/// building the registry when a test needs the text-check to pass.
pub fn seed_license_text(root: &Path, ids: &[&str]) {
    let dir = root.join("LICENSES");
    std::fs::create_dir_all(&dir).expect("create LICENSES dir");
    for id in ids {
        std::fs::write(dir.join(format!("{id}.txt")), "license body").expect("write LICENSES file");
    }
}

/// Seed a complete on-disk permissive license under `root`: both the grid
/// (`LICENSES/<id>.toml`) and the text (`LICENSES/<id>.txt`).
///
/// CLI/run tests that need a real `LicenseRegistry::load` to succeed use this.
pub fn seed_license(root: &Path, id: &str) {
    let dir = root.join("LICENSES");
    std::fs::create_dir_all(&dir).expect("create LICENSES dir");
    LicenseRegistry::builder()
        .license(LicenseSpec::new(id))
        .commit(root, &real_fs())
        .expect("seed license commit");
    seed_license_text(root, &[id]);
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

// --- License term fixtures (archetypes from the terms-redesign dialectic) ---

/// The "use however you want" baseline: all permissions granted, no obligations,
/// derivatives allowed, no manual review. The starting point most tests restrict from.
#[must_use]
pub fn permissive_terms() -> LicenseTerms {
    LicenseTerms::permissive()
}

/// Share-alike license (OFL, GPL, CC-BY-SA): derivatives allowed only under the
/// same license. Otherwise permissive.
#[must_use]
pub fn share_alike_terms() -> LicenseTerms {
    LicenseTerms::permissive().with_derivatives(Derivatives::ShareAlike)
}

/// No-derivatives license (CC-BY-ND): derivatives forbidden. Otherwise permissive.
#[must_use]
pub fn no_derivatives_terms() -> LicenseTerms {
    LicenseTerms::permissive().with_derivatives(Derivatives::Disallowed)
}

/// Non-commercial license (CC-BY-NC): commercial use forbidden. Otherwise permissive.
#[must_use]
pub fn non_commercial_terms() -> LicenseTerms {
    let mut t = LicenseTerms::permissive();
    t.allows_commercial_use = false;
    t
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
