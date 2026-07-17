//! `auditah audit` — obligation-aware compliance checking.
//!
//! Orchestrates discovery (cascade descent that inherits attribution records
//! top-down), resolution, and obligation verification against the license
//! registry. Produces [`Verdict`]s that the async pipeline routes into the
//! FAILED / ACCEPTED / error buckets; the synchronous `run_audit` collapses
//! the same kernel into an [`AuditReport`].

pub mod cascade;
pub mod pipeline;
pub mod report;

use std::path::Path;

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::audit::cascade::{descend, AuditInput, DirResult};
use crate::discovery::enumerator::ExcludeMatcher;
use crate::discovery::resolver::{ResolutionSource, ResolvedAsset};
use crate::model::terms::effective_terms;
use crate::services::Services;

use report::{Finding, FindingCode, Verdict};

/// Error running the audit pipeline.
#[derive(Debug, Error)]
#[error(debug)]
pub struct AuditError;

/// Run the full audit pipeline over the project root as a synchronous depth-first
/// cascade. Each directory is listed once; the inherited attribution record
/// descends from root and is fully replaced by any local `_manifest.toml`.
///
/// Returns the aggregate report. An `Ok` report may still contain `Fail`
/// findings (those are compliance problems, not pipeline failures).
///
/// # Errors
///
/// Returns `AuditError` if the root directory cannot be listed or a local
/// manifest cannot be read or parsed.
pub fn run_audit(services: &Services) -> Result<report::AuditReport, Report<AuditError>> {
    let root = services.config.root();
    let excludes = build_excludes(services)?;
    let mut report = report::AuditReport::default();
    let mut inputs: Vec<AuditInput> = Vec::new();
    cascade_collect(services, root, root, &excludes, None, &mut inputs)?;
    for input in inputs {
        match input {
            AuditInput::Asset(resolved) => {
                for verdict in audit_asset(&resolved, services) {
                    push_verdict(verdict, &mut report);
                }
            }
            AuditInput::Orphan(path) => {
                push_verdict(orphan_verdict(&path), &mut report);
            }
        }
    }
    Ok(report)
}

/// Append a [`Verdict`] to the report. `Accepted` is informational only; the
/// report tracks failures for exit-code purposes.
fn push_verdict(verdict: Verdict, report: &mut report::AuditReport) {
    match verdict {
        Verdict::Accepted(_) => {}
        Verdict::Failed(finding) => report.push(finding),
        Verdict::Error(path, detail) => {
            report.push_error(path, detail);
        }
    }
}

/// Depth-first descent over the cascade, flattening every directory's
/// [`AuditInput`]s (assets + orphans) into one ordered list. Recurses into
/// each non-excluded subdirectory with the effective inherited record.
///
/// # Errors
///
/// Returns `AuditError` if a directory cannot be listed or a local manifest
/// is unreadable/unparseable — the offending subtree is skipped (its inputs
/// are not collected), siblings continue, and the error is surfaced to the
/// caller as a single `AuditError`.
fn cascade_collect(
    services: &Services,
    dir: &Path,
    root: &Path,
    excludes: &ExcludeMatcher,
    inherited: Option<crate::audit::cascade::Inherited>,
    out: &mut Vec<AuditInput>,
) -> Result<(), Report<AuditError>> {
    let DirResult {
        assets,
        orphans,
        effective,
        subdirs,
    } = descend(services, dir, root, excludes, inherited)?;
    for resolved in assets {
        out.push(AuditInput::Asset(resolved));
    }
    for orphan in orphans {
        out.push(AuditInput::Orphan(orphan));
    }
    for subdir in subdirs {
        cascade_collect(services, &subdir, root, excludes, effective.clone(), out)?;
    }
    Ok(())
}

/// Build the exclude matcher from default + user-supplied globs.
///
/// Defense in depth: [`Config::load`] validates globs eagerly, but a
/// `Config` constructed directly (e.g. in tests) bypasses that, so this stays
/// fallible and propagates the error rather than panicking.
///
/// # Errors
/// Build the exclude matcher from the user config's exclude globs merged with
/// the built-in defaults. Shared by the sync and async audit paths.
///
pub(crate) fn build_excludes(services: &Services) -> Result<ExcludeMatcher, Report<AuditError>> {
    let patterns = crate::discovery::all_excludes(&services.config.config().exclude);
    ExcludeMatcher::new(&patterns)
        .change_context(AuditError)
        .attach("invalid exclude glob in auditah.toml")
}

/// Audit one resolved asset into a list of verdicts. A clean asset produces a
/// single [`Verdict::Accepted`]; a failing check produces one
/// [`Verdict::Failed`] per failed obligation (an asset can fail more than one).
/// Pure: no I/O, no mutation, deterministic given the registry + config.
#[must_use]
pub fn audit_asset(resolved: &ResolvedAsset, services: &Services) -> Vec<Verdict> {
    let asset = &resolved.asset_path;
    let mut findings: Vec<Finding> = Vec::new();

    check_coverage(asset, &resolved.source, &mut findings);
    let Some(record) = &resolved.record else {
        return findings_into_verdicts(asset, findings);
    };

    check_resolution(asset, record.license.as_str(), services, &mut findings);
    let Some(entry) = services.registry.get(&record.license) else {
        return findings_into_verdicts(asset, findings);
    };
    check_license_text(asset, &entry.id, services, &mut findings);
    let terms = effective_terms(&entry.terms, &record.overrides);
    check_obligations(asset, record, &entry.id, &terms, services, &mut findings);
    findings_into_verdicts(asset, findings)
}

/// Map a pile of findings into verdicts: no findings → one `Accepted`;
/// one or more → one `Failed` per finding.
fn findings_into_verdicts(asset: &Path, findings: Vec<Finding>) -> Vec<Verdict> {
    if findings.is_empty() {
        vec![Verdict::Accepted(asset.to_path_buf())]
    } else {
        findings.into_iter().map(Verdict::Failed).collect()
    }
}

/// A verdict for an orphan sidecar: always a single FAIL.
fn orphan_verdict(path: &Path) -> Verdict {
    Verdict::Failed(Finding::fail(
        FindingCode::OrphanSidecar,
        path.to_path_buf(),
        format!("orphan sidecar: no asset file for {}", path.display()),
    ))
}

/// Coverage: an asset with no resolvable config is unlicensed.
fn check_coverage(asset: &Path, source: &ResolutionSource, findings: &mut Vec<Finding>) {
    if matches!(source, ResolutionSource::None) {
        findings.push(Finding::fail(
            FindingCode::UnlicensedAsset,
            asset.to_path_buf(),
            format!(
                "unlicensed asset: no sidecar or manifest covers {}",
                asset.display()
            ),
        ));
    }
}

/// Resolution: the declared license must exist in the registry.
fn check_resolution(
    asset: &Path,
    license_id: &str,
    services: &Services,
    findings: &mut Vec<Finding>,
) {
    if services.registry.get(license_id).is_none() {
        findings.push(Finding::fail(
            FindingCode::UnknownLicense,
            asset.to_path_buf(),
            format!("unknown license id {license_id:?} not in registry"),
        ));
    }
}

/// License text presence: the referenced license must have a LICENSES/<id>.txt file.
fn check_license_text(
    asset: &Path,
    license_id: &str,
    services: &Services,
    findings: &mut Vec<Finding>,
) {
    let text_path = services
        .config
        .root()
        .join("LICENSES")
        .join(format!("{license_id}.txt"));
    if !services.fs.exists(&text_path) {
        findings.push(Finding::fail(
            FindingCode::MissingLicenseText,
            asset.to_path_buf(),
            format!(
                "license {license_id:?} has no LICENSES/{license_id}.txt; create it with the full license text"
            ),
        ));
    }
}

/// Obligation verification: audit either passes or fails. Checkable
/// obligations produce a Fail finding; obligations that cannot be
/// auto-verified (source disclosure, share-alike, license notice) produce
/// no finding here — they are documented on the terms, auto-complied by
/// `credits`/`NOTICES`, or gated by `manual_review`.
fn check_obligations(
    asset: &Path,
    record: &crate::model::attribution::AttributionRecord,
    license_id: &str,
    terms: &crate::model::terms::LicenseTerms,
    services: &Services,
    findings: &mut Vec<Finding>,
) {
    check_attribution(asset, record, terms, findings);
    check_commercial_boundary(asset, terms, services, findings);
    check_redistribution_boundary(asset, terms, services, findings);
    check_derivatives_boundary(asset, record, terms, findings);
    check_manual_review(asset, license_id, terms, services, findings);
}

/// Attribution: needs title + author + source when the obligation is set.
fn check_attribution(
    asset: &Path,
    record: &crate::model::attribution::AttributionRecord,
    terms: &crate::model::terms::LicenseTerms,
    findings: &mut Vec<Finding>,
) {
    if !terms.requires_attribution {
        return;
    }
    let missing_field = if record.title.trim().is_empty() {
        Some("title")
    } else if record.author.trim().is_empty() {
        Some("author")
    } else if record.source.trim().is_empty() {
        Some("source")
    } else {
        None
    };
    if let Some(field) = missing_field {
        findings.push(Finding::fail(
            FindingCode::IncompleteAttribution,
            asset.to_path_buf(),
            format!("license requires attribution but {field} is missing"),
        ));
    }
}

/// Commercial use boundary: a commercial project cannot use non-commercial assets.
fn check_commercial_boundary(
    asset: &Path,
    terms: &crate::model::terms::LicenseTerms,
    services: &Services,
    findings: &mut Vec<Finding>,
) {
    let config = services.config.config();
    if config.commercial_project && !terms.allows_commercial_use {
        findings.push(Finding::fail(
            FindingCode::NotCommerciallyLicensed,
            asset.to_path_buf(),
            "project is commercial but asset is not licensed for commercial use",
        ));
    }
}

/// Redistribution boundary: a redistributing project cannot use no-redistribution assets.
fn check_redistribution_boundary(
    asset: &Path,
    terms: &crate::model::terms::LicenseTerms,
    services: &Services,
    findings: &mut Vec<Finding>,
) {
    let config = services.config.config();
    if config.redistributes_assets && !terms.allows_redistribution {
        findings.push(Finding::fail(
            FindingCode::RedistributionViolation,
            asset.to_path_buf(),
            "project redistributes assets but license forbids redistribution",
        ));
    }
}

/// Derivatives boundary: a single exhaustive match over the [`Derivatives`] enum.
///
/// `Disallowed` + `modified` is a Fail; `Allowed` and `ShareAlike` are clean.
/// The `ShareAlike` variant is retained to document the obligation and drive
/// `credits`/`NOTICES`/`bom`; it produces no audit finding. No dead branches —
/// the match is exhaustive by construction.
fn check_derivatives_boundary(
    asset: &Path,
    record: &crate::model::attribution::AttributionRecord,
    terms: &crate::model::terms::LicenseTerms,
    findings: &mut Vec<Finding>,
) {
    use crate::model::terms::Derivatives;
    match terms.derivatives {
        Derivatives::Disallowed => {
            if record.modified {
                findings.push(Finding::fail(
                    FindingCode::ModifiedUnderNoDerivatives,
                    asset.to_path_buf(),
                    "asset is modified but license disallows derivatives",
                ));
            }
        }
        Derivatives::Allowed | Derivatives::ShareAlike => {}
    }
}

/// Manual review: fail-closed. A license marked `manual_review` FAILs the audit
/// until its id is listed in `manual_review_acknowledged` in the project config.
fn check_manual_review(
    asset: &Path,
    license_id: &str,
    terms: &crate::model::terms::LicenseTerms,
    services: &Services,
    findings: &mut Vec<Finding>,
) {
    let config = services.config.config();
    let acknowledged = config
        .manual_review_acknowledged
        .iter()
        .any(|id| id == license_id);
    if terms.manual_review && !acknowledged {
        findings.push(Finding::fail(
            FindingCode::ManualReviewRequired,
            asset.to_path_buf(),
            format!(
                "license {license_id:?} requires manual review; add it to `manual_review_acknowledged` in auditah.toml after review"
            ),
        ));
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::report::Verdict;
    use crate::config::Config;
    use crate::discovery::resolver::{ResolutionSource, ResolvedAsset};
    use crate::model::attribution::AttributionRecord;
    use crate::model::terms::{Derivatives, LicenseTerms, Overrides};
    use crate::registry::{LicenseRegistry, LicenseRegistryService, LicenseSpec};
    use crate::services::fs::FsService;
    use crate::services::Services;
    use crate::test_support::FakeFs;
    use std::sync::Arc;

    // --- shared fixtures ---

    fn default_config() -> Config {
        Config {
            commercial_project: false,
            redistributes_assets: false,
            manual_review_acknowledged: Vec::new(),
            exclude: Vec::new(),
        }
    }

    // Build a fake Services seeded with per-license text files on demand. The
    // audit_asset tests resolve LICENSES/<id>.txt; this helper stages a text
    // file for each id the test will reference.
    fn services_with_files(registry: &LicenseRegistry, config: Config, files: &[&str]) -> Services {
        let files: Vec<(String, &str)> = files
            .iter()
            .map(|f| (format!("/proj/{f}"), "text"))
            .collect();
        let fs = FsService::new(Arc::new(FakeFs::with_files(files)));
        Services::test()
            .fs(fs)
            .registry(LicenseRegistryService::new(Arc::new(registry.clone())))
            .config_root(std::path::Path::new("/proj"), config)
            .build()
    }

    fn clean_record(license: &str) -> AttributionRecord {
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

    fn resolved(asset: &str, record: Option<AttributionRecord>) -> ResolvedAsset {
        // A resolved record implies a non-None source (sidecar or manifest);
        // an absent record implies None. The coverage check keys off `source`,
        // so the two must stay consistent in fixtures.
        let source = if record.is_some() {
            ResolutionSource::Manifest(std::path::PathBuf::from("/proj/_manifest.toml"))
        } else {
            ResolutionSource::None
        };
        ResolvedAsset {
            asset_path: std::path::PathBuf::from(asset),
            record,
            source,
        }
    }

    fn code(verdict: &Verdict) -> Option<FindingCode> {
        match verdict {
            Verdict::Failed(f) => Some(f.code),
            _ => None,
        }
    }

    // --- one behavior per test ---

    #[test]
    fn clean_asset_is_accepted() {
        // Given a permissive license in the registry and its text on disk.
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-Mit"))
            .build();
        let services =
            services_with_files(&reg, default_config(), &["LICENSES/LicenseRef-Mit.txt"]);

        // When auditing a fully-covered asset.
        let v = audit_asset(
            &resolved("/proj/a.glb", Some(clean_record("LicenseRef-Mit"))),
            &services,
        );

        // Then the only verdict is Accepted.
        assert!(matches!(v.as_slice(), [Verdict::Accepted(_)]));
    }

    #[test]
    fn uncovered_asset_fails_unlicensed() {
        // Given an asset with no record (no cascade reaches it).
        let services = services_with_files(&LicenseRegistry::empty(), default_config(), &[]);

        // When auditing it.
        let v = audit_asset(&resolved("/proj/a.glb", None), &services);

        // Then it fails as UnlicensedAsset.
        assert!(v
            .iter()
            .any(|x| code(x) == Some(FindingCode::UnlicensedAsset)));
    }

    #[test]
    fn unknown_license_fails() {
        // Given an empty registry (the declared license won't resolve).
        let services = services_with_files(&LicenseRegistry::empty(), default_config(), &[]);

        // When auditing an asset declaring a license absent from the registry.
        let v = audit_asset(
            &resolved("/proj/a.glb", Some(clean_record("LicenseRef-Ghost"))),
            &services,
        );

        // Then it fails as UnknownLicense.
        assert!(v
            .iter()
            .any(|x| code(x) == Some(FindingCode::UnknownLicense)));
    }

    #[test]
    fn missing_license_text_fails() {
        // Given a registered license with NO text file on disk.
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-Mit"))
            .build();
        let services = services_with_files(&reg, default_config(), &[]);

        // When auditing an asset under that license.
        let v = audit_asset(
            &resolved("/proj/a.glb", Some(clean_record("LicenseRef-Mit"))),
            &services,
        );

        // Then it fails as MissingLicenseText.
        assert!(v
            .iter()
            .any(|x| code(x) == Some(FindingCode::MissingLicenseText)));
    }

    #[test]
    fn incomplete_attribution_fails() {
        // Given a license that requires attribution.
        let mut terms = LicenseTerms::permissive();
        terms.requires_attribution = true;
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-By").terms(terms))
            .build();
        let services = services_with_files(&reg, default_config(), &["LICENSES/LicenseRef-By.txt"]);

        // When auditing an asset whose record omits the author.
        let mut rec = clean_record("LicenseRef-By");
        rec.author.clear();
        let v = audit_asset(&resolved("/proj/a.glb", Some(rec)), &services);

        // Then it fails as IncompleteAttribution.
        assert!(v
            .iter()
            .any(|x| code(x) == Some(FindingCode::IncompleteAttribution)));
    }

    #[test]
    fn non_commercial_under_commercial_project_fails() {
        // Given a commercial config and a non-commercial license.
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-Nc").terms(non_commercial_terms()))
            .build();
        let mut config = default_config();
        config.commercial_project = true;
        let services = services_with_files(&reg, config, &["LICENSES/LicenseRef-Nc.txt"]);

        // When auditing an asset under the non-commercial license.
        let v = audit_asset(
            &resolved("/proj/a.glb", Some(clean_record("LicenseRef-Nc"))),
            &services,
        );

        // Then it fails as NotCommerciallyLicensed.
        assert!(v
            .iter()
            .any(|x| code(x) == Some(FindingCode::NotCommerciallyLicensed)));
    }

    #[test]
    fn no_redistribution_under_redistributing_project_fails() {
        // Given a redistributing config and a no-redistribution license.
        let mut terms = LicenseTerms::permissive();
        terms.allows_redistribution = false;
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-NoRed").terms(terms))
            .build();
        let mut config = default_config();
        config.redistributes_assets = true;
        let services = services_with_files(&reg, config, &["LICENSES/LicenseRef-NoRed.txt"]);

        // When auditing an asset under that license.
        let v = audit_asset(
            &resolved("/proj/a.glb", Some(clean_record("LicenseRef-NoRed"))),
            &services,
        );

        // Then it fails as RedistributionViolation.
        assert!(v
            .iter()
            .any(|x| code(x) == Some(FindingCode::RedistributionViolation)));
    }

    #[test]
    fn modified_under_no_derivatives_fails() {
        // Given a no-derivatives license.
        let reg = LicenseRegistry::builder()
            .license(
                LicenseSpec::new("LicenseRef-Nd")
                    .terms(LicenseTerms::permissive().with_derivatives(Derivatives::Disallowed)),
            )
            .build();
        let services = services_with_files(&reg, default_config(), &["LICENSES/LicenseRef-Nd.txt"]);

        // When auditing a modified asset under that license.
        let mut rec = clean_record("LicenseRef-Nd");
        rec.modified = true;
        let v = audit_asset(&resolved("/proj/a.glb", Some(rec)), &services);

        // Then it fails as ModifiedUnderNoDerivatives.
        assert!(v
            .iter()
            .any(|x| code(x) == Some(FindingCode::ModifiedUnderNoDerivatives)));
    }

    #[test]
    fn manual_review_license_fails_until_acknowledged() {
        // Given a license flagged for manual review and a config that does NOT acknowledge it.
        let mut terms = LicenseTerms::permissive();
        terms.manual_review = true;
        let reg = LicenseRegistry::builder()
            .license(LicenseSpec::new("LicenseRef-Mr").terms(terms))
            .build();
        let services = services_with_files(&reg, default_config(), &["LICENSES/LicenseRef-Mr.txt"]);

        // When auditing an asset under that license.
        let v = audit_asset(
            &resolved("/proj/a.glb", Some(clean_record("LicenseRef-Mr"))),
            &services,
        );

        // Then it fails as ManualReviewRequired.
        assert!(v
            .iter()
            .any(|x| code(x) == Some(FindingCode::ManualReviewRequired)));
    }

    #[test]
    fn orphan_sidecar_yields_failed_verdict() {
        // Given an orphan sidecar path.
        let path = std::path::PathBuf::from("/proj/ghost.glb.attr.toml");

        // When mapping it to its verdict.
        let v = orphan_verdict(&path);

        // Then it is a Failed OrphanSidecar.
        assert!(matches!(&v, Verdict::Failed(f) if f.code == FindingCode::OrphanSidecar));
    }

    fn non_commercial_terms() -> LicenseTerms {
        let mut t = LicenseTerms::permissive();
        t.allows_commercial_use = false;
        t
    }
}
