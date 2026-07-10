//! Audit report types: findings, severity, and the aggregate report.

use std::path::PathBuf;

/// Severity of an audit finding.
///
/// `Fail` blocks compliance (non-zero exit). `Flag` surfaces a condition that
/// cannot be auto-verified and needs human review (e.g. share-alike clauses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Blocking: compliance is violated. Auditor can prove it.
    Fail,
    /// Non-blocking: needs human review. Auditor cannot auto-verify.
    Flag,
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
    /// `allows_modifications = false` (effective) but `modified = true`.
    ModifiedUnderNoDerivatives,
    /// `requires_share_alike` — human must confirm distribution license.
    ShareAlikeReview,
    /// `requires_source_disclosure` — human must confirm source offering.
    SourceDisclosureReview,
    /// `requires_license_notice` — human must confirm license text shipped.
    LicenseNoticeReview,
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

    /// Convenience constructor for a `Flag`.
    #[must_use]
    pub fn flag(code: FindingCode, asset: PathBuf, detail: impl Into<String>) -> Self {
        Self {
            severity: Severity::Flag,
            code,
            asset,
            detail: detail.into(),
        }
    }
}

/// The aggregate audit result.
#[derive(Debug, Default)]
pub struct AuditReport {
    pub findings: Vec<Finding>,
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

    /// Count of `Flag` findings.
    #[must_use]
    pub fn flag_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Flag)
            .count()
    }

    /// Add a finding.
    pub fn push(&mut self, finding: Finding) {
        self.findings.push(finding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset() -> PathBuf {
        PathBuf::from("/proj/sword.glb")
    }

    #[test]
    fn empty_report_has_no_failures() {
        let r = AuditReport::default();
        assert!(!r.has_failures());
        assert_eq!(r.fail_count(), 0);
        assert_eq!(r.flag_count(), 0);
    }

    #[test]
    fn flag_only_does_not_count_as_failure() {
        let mut r = AuditReport::default();
        r.push(Finding::flag(
            FindingCode::ShareAlikeReview,
            asset(),
            "review",
        ));
        assert!(!r.has_failures());
        assert_eq!(r.flag_count(), 1);
        assert_eq!(r.fail_count(), 0);
    }

    #[test]
    fn fail_marks_has_failures_true() {
        let mut r = AuditReport::default();
        r.push(Finding::fail(
            FindingCode::UnlicensedAsset,
            asset(),
            "uncovered",
        ));
        assert!(r.has_failures());
        assert_eq!(r.fail_count(), 1);
    }

    #[test]
    fn mixed_fail_and_flag_counted_separately() {
        let mut r = AuditReport::default();
        r.push(Finding::fail(FindingCode::UnknownLicense, asset(), "x"));
        r.push(Finding::flag(FindingCode::LicenseNoticeReview, asset(), "y"));
        r.push(Finding::fail(FindingCode::IncompleteAttribution, asset(), "z"));
        assert_eq!(r.fail_count(), 2);
        assert_eq!(r.flag_count(), 1);
        assert!(r.has_failures());
    }
}
