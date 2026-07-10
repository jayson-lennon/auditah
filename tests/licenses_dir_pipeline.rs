//! Integration tests: LICENSES directory support.
//!
//! Covers the new obligation that every referenced license id must have a
//! `LICENSES/<id>.txt` file on disk, and that `init-licenses` generates them.

use std::sync::Arc;

use auditah::audit::report::{FindingCode, Severity};
use auditah::audit::{run_audit, AuditCtx};
use auditah::config::Config;
use auditah::init_licenses::init_licenses;
use auditah::registry::LicenseRegistry;
use auditah::services::fs::{FsService, RealFs};
use auditah::services::Services;
use temptree::temptree;

fn services() -> Services {
    Services {
        fs: FsService::new(Arc::new(RealFs::new())),
        registry: LicenseRegistry::embedded_only(),
    }
}

fn config() -> Config {
    Config {
        commercial_project: false,
        exclude: Vec::new(),
    }
}

fn codes_for(report: &auditah::audit::report::AuditReport, needle: &str) -> Vec<FindingCode> {
    report
        .findings
        .iter()
        .filter(|f| f.asset.to_string_lossy().contains(needle))
        .map(|f| f.code)
        .collect()
}

// A covered CC-BY asset with no LICENSES/ directory → FAIL MissingLicenseText.
#[test]
fn audit_fails_when_license_text_missing() {
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
    let report = run_audit(&ctx).unwrap();
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
    let tree = temptree! {
        "sack.glb": "binary",
        "sack.glb.attr.toml": "title = \"Sack\"\nauthor = \"A\"\nyear = 2019\nlicense = \"CC-BY-3.0\"\nsource = \"https://x\"\n",
    };
    let root = tree.path();
    let svc = services();
    let cfg = config();

    // Generate license text files.
    let outcome = init_licenses(&svc, root).unwrap();
    assert!(
        outcome.written >= 1,
        "expected at least CC-BY-3.0.txt written"
    );

    // Audit should now pass clean.
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

    // init-licenses should write LICENSES/LicenseRef-Custom.txt from inline text.
    let outcome = init_licenses(&svc, root).unwrap();
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

    // Audit now passes (text file present).
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
    // No LICENSES/ yet → FAIL.
    let report = run_audit(&ctx).unwrap();
    let codes = codes_for(&report, "rock.glb");
    assert!(
        codes.contains(&FindingCode::MissingLicenseText),
        "CC0 must also require its text file, got {codes:?}"
    );

    // After init, clean.
    init_licenses(&svc, root).unwrap();
    let report = run_audit(&ctx).unwrap();
    assert!(
        !report.has_failures(),
        "expected clean after init-licenses, got: {:?}",
        report.findings
    );
    let _ = Severity::Fail; // keep import used
}
