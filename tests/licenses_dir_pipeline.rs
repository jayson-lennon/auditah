//! Integration tests: LICENSES directory support.
//!
//! Covers the new obligation that every referenced license id must have a
//! `LICENSES/<id>.txt` file on disk, and that `init-licenses` generates them.

use auditah::audit::report::{FindingCode, Severity};
use auditah::audit::{run_audit, AuditCtx};
use auditah::init_licenses::init_licenses;
use temptree::temptree;

mod common;
use auditah::registry::LicenseRegistry;
use auditah::services::fs::{FsService, RealFs};
use auditah::services::Services;
use common::{codes_for, config, services};
use std::sync::Arc;

// A covered CC-BY asset with no LICENSES/ directory → FAIL MissingLicenseText.
#[test]
fn audit_fails_when_license_text_missing() {
    // Given a CC-BY asset with no LICENSES/ directory.
    let tree = temptree! {
        "sack.glb": "binary",
        "sack.glb.attr.toml": "title = \"Sack\"\nauthor = \"A\"\nyear = 2019\nlicense = \"CC-BY-3.0\"\nsource = \"https://x\"\n",
    };
    let root = tree.path();
    let svc = services();
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
    let codes = codes_for(&report, "sack.glb");
    assert!(
        codes.contains(&FindingCode::MissingLicenseText),
        "expected MissingLicenseText, got {codes:?}"
    );
}

// After init-licenses writes LICENSES/CC-BY-3.0.txt, audit passes clean.
#[test]
fn init_licenses_makes_audit_pass() {
    // Given a CC-BY asset with no LICENSES/ directory.
    let tree = temptree! {
        "sack.glb": "binary",
        "sack.glb.attr.toml": "title = \"Sack\"\nauthor = \"A\"\nyear = 2019\nlicense = \"CC-BY-3.0\"\nsource = \"https://x\"\n",
    };
    let root = tree.path();
    let svc = services();
    let cfg = config();

    // When running init-licenses.
    let outcome = init_licenses(&svc, root).unwrap();

    // Then at least CC-BY-3.0.txt is written.
    assert!(
        outcome.written >= 1,
        "expected at least CC-BY-3.0.txt written"
    );

    // And the audit now passes clean.
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };
    let report = run_audit(&ctx).unwrap();
    assert!(
        !report.has_failures(),
        "expected clean audit after init-licenses, got: {:?}",
        report.findings
    );
}

// A hand-authored custom LicenseRef must have its own LICENSES text written by
// init-licenses (sourced from the project-local .toml inline `text`).
#[test]
fn custom_licenseref_text_written_from_project_local_toml() {
    // Given a CC-BY asset and a project-local custom LicenseRef with inline text.
    let tree = temptree! {
        "statue.glb": "binary",
        "statue.glb.attr.toml": "title = \"Statue\"\nauthor = \"S\"\nyear = 2020\nlicense = \"LicenseRef-Custom\"\nsource = \"https://x\"\n",
        "licenses": {
            "LicenseRef-Custom.toml": r#"
id = "LicenseRef-Custom"
name = "Custom Studio License"
url = "https://example.com/custom"
text = "CUSTOM LICENSE TEXT BODY"
[terms]
requires_attribution = true
requires_license_notice = false
requires_source_disclosure = false
requires_share_alike = false
requires_modification_notice = false
allows_commercial_use = true
allows_modifications = true
"#,
        },
    };
    let root = tree.path();
    let fs = FsService::new(Arc::new(RealFs::new()));
    let registry = LicenseRegistry::load(&fs, root).unwrap();
    let svc = Services { fs, registry };
    let cfg = config();

    // When running init-licenses.
    let outcome = init_licenses(&svc, root).unwrap();

    // Then the custom license text is written from the inline text, and audit passes.
    let custom_path = root.join("LICENSES").join("LicenseRef-Custom.txt");
    assert!(
        custom_path.exists(),
        "custom license text should be written"
    );
    let written = std::fs::read_to_string(&custom_path).unwrap();
    assert!(
        written.contains("CUSTOM LICENSE TEXT BODY"),
        "custom text should come from project-local toml inline text"
    );
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };
    let report = run_audit(&ctx).unwrap();
    assert!(
        !report.has_failures(),
        "expected clean audit for custom license with text, got: {:?}",
        report.findings
    );
    let _ = outcome;
}

// CC0 assets (no attribution) still require LICENSES/CC0-1.0.txt.
#[test]
fn cc0_asset_also_requires_license_text() {
    // Given a CC0 asset with no LICENSES/ directory.
    let tree = temptree! {
        "rock.glb": "binary",
        "rock.glb.attr.toml": "title = \"Rock\"\nauthor = \"A\"\nyear = 2020\nlicense = \"CC0-1.0\"\nsource = \"https://x\"\n",
    };
    let root = tree.path();
    let svc = services();
    let cfg = config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit before init-licenses.
    let report = run_audit(&ctx).unwrap();

    // Then CC0 FAILs as MissingLicenseText (it also requires its text file).
    let codes = codes_for(&report, "rock.glb");
    assert!(
        codes.contains(&FindingCode::MissingLicenseText),
        "CC0 must also require its text file, got {codes:?}"
    );

    // When running init-licenses then re-auditing.
    init_licenses(&svc, root).unwrap();
    let report = run_audit(&ctx).unwrap();

    // Then the audit is clean.
    assert!(
        !report.has_failures(),
        "expected clean after init-licenses, got: {:?}",
        report.findings
    );
    let _ = Severity::Fail; // keep import used
}
