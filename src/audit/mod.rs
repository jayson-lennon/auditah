//! `auditah audit` — obligation-aware compliance checking.
//!
//! Orchestrates discovery (enumerate assets), resolution (find each asset's
//! attribution config by precedence), and obligation verification against the
//! license registry. Produces an [`AuditReport`] of [`Finding`]s.

pub mod report;

use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::config::Config;
use crate::discovery::enumerator::{enumerate, ExcludeMatcher};
use crate::discovery::resolver::{find_orphan_sidecars, resolve, ResolutionSource};
use crate::model::terms::effective_terms;
use crate::services::Services;

use report::{AuditReport, Finding, FindingCode};

/// Error running the audit pipeline.
#[derive(Debug, Error)]
#[error(debug)]
pub struct AuditError;

/// Subsystem context: everything the audit subsystem needs to run.
/// Discovery + resolution + registry + config all flow through here.
#[derive(Debug, Clone)]
pub struct AuditCtx<'a> {
    pub services: &'a Services,
    pub config: &'a Config,
    pub root: &'a Path,
}

/// Run the full audit pipeline over `ctx.root`. Returns the aggregate report.
///
/// Errors mean the pipeline itself broke (walk/read failures); an `Ok` report
/// may still contain `Fail` findings.
///
/// # Errors
///
/// Returns `AuditError` if enumeration or resolution encounters an IO/parse failure.
pub fn run_audit(ctx: &AuditCtx) -> Result<AuditReport, Report<AuditError>> {
    let excludes = build_excludes(ctx);
    let assets = enumerate(&ctx.services.fs, ctx.root, &excludes)
        .change_context(AuditError)
        .attach("failed to enumerate assets")?;
    let all_files = ctx
        .services
        .fs
        .walk(ctx.root)
        .change_context(AuditError)
        .attach("failed to walk filesystem for orphan detection")?;

    let mut report = AuditReport::default();

    check_orphan_sidecars(&all_files, ctx, &mut report);
    for asset in &assets {
        let resolved = resolve(&ctx.services.fs, asset, ctx.root)
            .change_context(AuditError)
            .attach("failed to resolve asset config")?;
        check_coverage(asset, &resolved.source, &mut report);
        if let Some(record) = &resolved.record {
            check_resolution(asset, record.license.as_str(), ctx, &mut report);
            if let Some(entry) = ctx.services.registry.get(&record.license) {
                check_license_text(asset, &entry.id, ctx, &mut report);
                let terms = effective_terms(&entry.terms, &record.overrides);
                check_obligations(asset, record, &terms, ctx.config, &mut report);
            }
        }
    }
    Ok(report)
}

/// Build the exclude matcher from default + user-supplied globs.
fn build_excludes(ctx: &AuditCtx) -> ExcludeMatcher {
    let patterns = crate::discovery::all_excludes(&ctx.config.exclude);
    ExcludeMatcher::new(&patterns)
        .expect("default + user exclude patterns must compile (validated at config load)")
}

/// Surface orphan sidecars as Fail findings.
fn check_orphan_sidecars(all_files: &[PathBuf], ctx: &AuditCtx, report: &mut AuditReport) {
    for orphan in find_orphan_sidecars(&ctx.services.fs, all_files) {
        report.push(Finding::fail(
            FindingCode::OrphanSidecar,
            orphan.clone(),
            format!("orphan sidecar: no asset file for {}", orphan.display()),
        ));
    }
}

/// Coverage: an asset with no resolvable config is unlicensed.
fn check_coverage(asset: &Path, source: &ResolutionSource, report: &mut AuditReport) {
    if matches!(source, ResolutionSource::None) {
        report.push(Finding::fail(
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
fn check_resolution(asset: &Path, license_id: &str, ctx: &AuditCtx, report: &mut AuditReport) {
    if ctx.services.registry.get(license_id).is_none() {
        report.push(Finding::fail(
            FindingCode::UnknownLicense,
            asset.to_path_buf(),
            format!("unknown license id {license_id:?} not in registry"),
        ));
    }
}

/// License text presence: the referenced license must have a LICENSES/<id>.txt file.
fn check_license_text(asset: &Path, license_id: &str, ctx: &AuditCtx, report: &mut AuditReport) {
    let text_path = ctx.root.join("LICENSES").join(format!("{license_id}.txt"));
    if !ctx.services.fs.exists(&text_path) {
        report.push(Finding::fail(
            FindingCode::MissingLicenseText,
            asset.to_path_buf(),
            format!(
                "license {license_id:?} has no LICENSES/{license_id}.txt; run `auditah init-licenses`"
            ),
        ));
    }
}

/// Obligation verification: checkable obligations Fail; uncheckable ones Flag.
fn check_obligations(
    asset: &Path,
    record: &crate::model::attribution::AttributionRecord,
    terms: &crate::model::terms::LicenseTerms,
    config: &Config,
    report: &mut AuditReport,
) {
    // Attribution: needs title + author + source.
    if terms.requires_attribution {
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
            report.push(Finding::fail(
                FindingCode::IncompleteAttribution,
                asset.to_path_buf(),
                format!("license requires attribution but {field} is missing"),
            ));
        }
    }

    // Commercial use boundary.
    if config.commercial_project && !terms.allows_commercial_use {
        report.push(Finding::fail(
            FindingCode::NotCommerciallyLicensed,
            asset.to_path_buf(),
            "project is commercial but asset is not licensed for commercial use",
        ));
    }

    // Modifications boundary.
    if record.modified && !terms.allows_modifications {
        report.push(Finding::fail(
            FindingCode::ModifiedUnderNoDerivatives,
            asset.to_path_buf(),
            "asset is modified but license disallows derivatives",
        ));
    }

    // Manual-review flags: cannot auto-verify, surface for human action.
    if terms.requires_share_alike {
        report.push(Finding::flag(
            FindingCode::ShareAlikeReview,
            asset.to_path_buf(),
            "license requires share-alike; confirm distribution license compatibility",
        ));
    }
    if terms.requires_source_disclosure {
        report.push(Finding::flag(
            FindingCode::SourceDisclosureReview,
            asset.to_path_buf(),
            "license requires source disclosure; confirm source is offered on distribution",
        ));
    }
    if terms.requires_license_notice {
        report.push(Finding::flag(
            FindingCode::LicenseNoticeReview,
            asset.to_path_buf(),
            "license requires reproducing the license notice; confirm it ships",
        ));
    }
}
