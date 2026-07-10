//! Integration tests: LICENSES directory support.
//!
//! Covers the obligation that every referenced license id must have a
//! `LICENSES/<id>.txt` file on disk for the audit to pass. License grids
//! (`<id>.toml`) and text (`<id>.txt`) live together in `LICENSES/`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::audit::report::{FindingCode, Severity};
use auditah::audit::{run_audit, AuditCtx};
use auditah::registry::{LicenseRegistry, LicenseSpec};
use auditah::services::fs::{FsService, RealFs};
use auditah::services::Services;
use std::sync::Arc;
use temptree::temptree;

mod common;
use common::{codes_for, config};

fn services_with_license(license_id: &str) -> Services {
    let _ = license_id;
    let registry = LicenseRegistry::builder()
        .license(LicenseSpec::new("LicenseRef-Custom"))
        .build();
    Services {
        fs: FsService::new(Arc::new(RealFs::new())),
        registry,
    }
}

// A covered LicenseRef-Custom asset with no LICENSES/ directory → FAIL MissingLicenseText.
#[test]
fn audit_fails_when_license_text_missing() {
    // Given a LicenseRef-Custom asset with the grid but no LICENSES/<id>.txt.
    let tree = temptree! {
        "statue.glb": "binary",
        "statue.glb.attr.toml": "title = \"Statue\"\nauthor = \"S\"\nyear = 2020\nlicense = \"LicenseRef-Custom\"\nsource = \"https://x\"\n",
    };
    let root = tree.path();
    let svc = services_with_license("LicenseRef-Custom");
    let cfg = config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset FAILs as MissingLicenseText.
    assert!(report.has_failures());
    let codes = codes_for(&report, "statue.glb");
    assert!(
        codes.contains(&FindingCode::MissingLicenseText),
        "expected MissingLicenseText, got {codes:?}"
    );
}

// Once LICENSES/LicenseRef-Custom.txt is seeded, audit passes clean.
#[test]
fn audit_passes_when_license_text_present() {
    // Given a LicenseRef-Custom asset with both the grid and the .txt file.
    let tree = temptree! {
        "statue.glb": "binary",
        "statue.glb.attr.toml": "title = \"Statue\"\nauthor = \"S\"\nyear = 2020\nlicense = \"LicenseRef-Custom\"\nsource = \"https://x\"\n",
        "LICENSES": {
            "LicenseRef-Custom.txt": "CUSTOM LICENSE TEXT BODY",
        },
    };
    let root = tree.path();
    let svc = services_with_license("LicenseRef-Custom");
    let cfg = config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the audit is clean.
    assert!(
        !report.has_failures(),
        "expected clean audit with text present, got: {:?}",
        report.findings
    );
    let _ = Severity::Fail; // keep import used
}
