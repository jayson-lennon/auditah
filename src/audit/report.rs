//! Audit report types: findings, severity, and the aggregate report.

use std::path::PathBuf;

/// Severity of an audit finding.
///
/// `Fail` blocks compliance (non-zero exit). There is no non-blocking
/// severity: audit either passes or fails. Obligations that cannot be
/// auto-verified (source disclosure, share-alike, license-notice shipping)
/// are either auto-complied by `credits`/`NOTICES` or surfaced via the
/// `manual_review` forcing function — never as a silent warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Blocking: compliance is violated. Auditor can prove it.
    Fail,
}

/// Machine-readable code identifying the kind of finding. Drives grouping in
/// the report and makes assertions in tests precise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingCode {
    /// Asset has no sidecar and no enclosing manifest.
    UnlicensedAsset,
    /// A `<asset>.attr.toml` exists but `<asset>` does not.
    OrphanSidecar,
    /// `record.license` does not resolve in the registry.
    UnknownLicense,
    /// `requires_attribution` set but `title`/`author`/`source` missing.
    IncompleteAttribution,
    /// `allows_commercial_use = false` (effective) under a commercial project.
    NotCommerciallyLicensed,
    /// `derivatives = "disallowed"` (effective) but `modified = true`.
    ModifiedUnderNoDerivatives,
    /// Referenced license id has no `LICENSES/<id>.txt` file on disk.
    MissingLicenseText,
    /// `allows_redistribution = false` (effective) under a redistributing project.
    RedistributionViolation,
    /// `manual_review = true` and license id not in `manual_review_acknowledged`.
    ManualReviewRequired,
}

/// A single audit finding about one asset.
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub code: FindingCode,
    /// Path of the asset the finding concerns (or the orphan sidecar's path).
    pub asset: PathBuf,
    /// Human-readable detail.
    pub detail: String,
}

impl Finding {
    /// Convenience constructor for a `Fail`.
    #[must_use]
    pub fn fail(code: FindingCode, asset: PathBuf, detail: impl Into<String>) -> Self {
        Self {
            severity: Severity::Fail,
            code,
            asset,
            detail: detail.into(),
        }
    }
}

/// The outcome of auditing one asset: passed the checks, failed one or more,
/// or hit a technical error (unreadable manifest, etc.).
///
/// One collector task consumes these in the async pipeline; the reporter
/// routes them into the FAILED / ACCEPTED / error buckets for output.
#[derive(Debug, Clone)]
pub enum Verdict {
    /// Asset passed every applicable check.
    Accepted(PathBuf),
    /// Asset failed one or more checks.
    Failed(Finding),
    /// Technical failure tied to a path (e.g. a directory whose manifest
    /// could not be read). Printed dead last, never lost.
    Error(PathBuf, String),
}

/// The aggregate audit result.
#[derive(Debug, Default)]
pub struct AuditReport {
    pub findings: Vec<Finding>,
    /// Technical failures (unreadable manifest, walk fault, task panic), kept
    /// distinct from compliance findings so they surface as exit-2 errors,
    /// never as compliance FAILs and never lost.
    pub errors: Vec<(PathBuf, String)>,
}

impl AuditReport {
    /// Whether any blocking `Fail` finding is present.
    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Fail)
    }

    /// Count of `Fail` findings.
    #[must_use]
    pub fn fail_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Fail)
            .count()
    }

    /// Add a finding.
    pub fn push(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// Record a technical error tied to a path.
    pub fn push_error(&mut self, path: PathBuf, detail: impl Into<String>) {
        self.errors.push((path, detail.into()));
    }

    /// Whether any technical error was recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Count of technical errors.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> PathBuf {
        PathBuf::from("/proj/sword.glb")
    }

    #[test]
    fn empty_report_has_no_failures() {
        // Given nothing.
        // When constructing a default (empty) report.
        let r = AuditReport::default();

        // Then it has no failures and zero counts.
        assert!(!r.has_failures());
        assert_eq!(r.fail_count(), 0);
    }
    #[test]
    fn fail_marks_has_failures_true() {
        // Given a report containing a FAIL finding.
        let mut r = AuditReport::default();
        r.push(Finding::fail(
            FindingCode::UnlicensedAsset,
            asset(),
            "uncovered",
        ));

        // When inspecting the report.
        let has_failures = r.has_failures();

        // Then has_failures is true and fail_count is 1.
        assert!(has_failures);
        assert_eq!(r.fail_count(), 1);
    }

    #[test]
    fn multiple_fails_all_counted() {
        // Given a report with 2 fails.
        let mut r = AuditReport::default();
        r.push(Finding::fail(FindingCode::UnknownLicense, asset(), "x"));
        r.push(Finding::fail(
            FindingCode::IncompleteAttribution,
            asset(),
            "z",
        ));

        // When inspecting the report.
        let fail_count = r.fail_count();

        // Then fail_count is 2 and has_failures is true.
        assert_eq!(fail_count, 2);
        assert!(r.has_failures());
    }
    #[test]
    fn missing_license_text_is_a_fail() {
        // Given a report with a MissingLicenseText fail.
        let mut r = AuditReport::default();
        r.push(Finding::fail(
            FindingCode::MissingLicenseText,
            asset(),
            "no LICENSES/MIT.txt",
        ));

        // When inspecting the report.
        let has_failures = r.has_failures();

        // Then has_failures is true and fail_count is 1.
        assert!(has_failures);
        assert_eq!(r.fail_count(), 1);
    }

    #[test]
    fn technical_error_does_not_count_as_compliance_failure() {
        // Given a report with only a technical error (no findings).
        let mut r = AuditReport::default();
        r.push_error(asset(), "unreadable manifest");

        // When inspecting the report.
        // Then it has an error but is NOT a compliance failure — the two are
        // distinct buckets so technical faults surface as exit-2, not exit-1.
        assert!(r.has_errors());
        assert_eq!(r.error_count(), 1);
        assert!(!r.has_failures());
        assert_eq!(r.fail_count(), 0);
    }
}
