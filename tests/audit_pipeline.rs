//! Integration tests: the `audit` pipeline (`run_audit`) end-to-end against a
//! real temp filesystem. One BDD test per behavior, mapped to the plan's test
//! cases table.

use std::sync::Arc;

use auditah::audit::report::{FindingCode, Severity};
use auditah::audit::{run_audit, AuditCtx};
use auditah::config::Config;
use auditah::registry::LicenseRegistry;
use auditah::services::fs::{FsService, RealFs};
use auditah::services::Services;
use temptree::temptree;

mod common;
use common::{codes_for, non_commercial_config, seed_licenses, services};

// Test case 1: uncovered asset → FAIL UnlicensedAsset.
#[test]
fn uncovered_asset_fails_as_unlicensed() {
    // Given an uncovered asset with no sidecar or manifest.
    let tree = temptree! { "sword.glb": "binary" };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset FAILs as UnlicensedAsset.
    assert!(report.has_failures());
    let codes = codes_for(&report, "sword.glb");
    assert!(
        codes.contains(&FindingCode::UnlicensedAsset),
        "expected UnlicensedAsset, got {codes:?}"
    );
}

// Test case 2: orphan sidecar → FAIL OrphanSidecar.
#[test]
fn orphan_sidecar_fails() {
    // Given a sidecar whose asset does not exist.
    let tree = temptree! {
        "ghost.glb.attr.toml": "title = \"G\"\nauthor = \"A\"\nyear = 2020\nlicense = \"CC0-1.0\"\nsource = \"https://x\"\n",
        "real.glb": "binary",
        "real.glb.attr.toml": "title = \"R\"\nauthor = \"A\"\nyear = 2020\nlicense = \"CC0-1.0\"\nsource = \"https://x\"\n"
    };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the orphan sidecar FAILs as OrphanSidecar.
    let codes = codes_for(&report, "ghost");
    assert!(
        codes.contains(&FindingCode::OrphanSidecar),
        "expected OrphanSidecar, got {codes:?}"
    );
}

// Test case 6: unknown license id → FAIL UnknownLicense.
#[test]
fn unknown_license_id_fails() {
    // Given an asset whose sidecar references an unregistered license id.
    let tree = temptree! {
        "rock.glb": "binary",
        "rock.glb.attr.toml": r#"
title = "Rock"
author = "A"
year = 2020
license = "GPL-3.0"
source = "https://example.com"
"#
    };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset FAILs as UnknownLicense.
    let codes = codes_for(&report, "rock.glb");
    assert!(
        codes.contains(&FindingCode::UnknownLicense),
        "expected UnknownLicense, got {codes:?}"
    );
}

// Test case 7: requires_attribution + missing source → FAIL IncompleteAttribution.
#[test]
fn incomplete_attribution_missing_source_fails() {
    // Given a CC-BY asset whose sidecar has an empty source.
    let tree = temptree! {
        "tile.glb": "binary",
        "tile.glb.attr.toml": r#"
title = "Tile"
author = "Quaternius"
year = 2022
license = "CC-BY-3.0"
source = ""
"#
    };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset FAILs as IncompleteAttribution.
    let codes = codes_for(&report, "tile.glb");
    assert!(
        codes.contains(&FindingCode::IncompleteAttribution),
        "expected IncompleteAttribution, got {codes:?}"
    );
}

// Test case 8: allows_commercial_use=false (via override) + commercial_project → FAIL.
#[test]
fn non_commercial_asset_fails_under_commercial_project() {
    // Given a CC-BY asset overridden to non-commercial under a commercial project.
    let tree = temptree! {
        "fanfare.ogg": "binary",
        "fanfare.ogg.attr.toml": r#"
title = "Fanfare"
author = "Musician"
year = 2021
license = "CC-BY-3.0"
source = "https://example.com"

[overrides]
allows_commercial_use = false
"#
    };
    let root = tree.path();
    let svc = services();
    let cfg = Config {
        commercial_project: true,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    };
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset FAILs as NotCommerciallyLicensed.
    let codes = codes_for(&report, "fanfare");
    assert!(
        codes.contains(&FindingCode::NotCommerciallyLicensed),
        "expected NotCommerciallyLicensed, got {codes:?}"
    );
}

// Test case 9: derivatives="disallowed" (via override) + modified=true → FAIL.
#[test]
fn modified_under_no_derivatives_fails() {
    // Given a modified asset overridden to no-derivatives.
    let tree = temptree! {
        "statue.glb": "binary",
        "statue.glb.attr.toml": r#"
title = "Statue"
author = "Sculptor"
year = 2019
license = "CC-BY-3.0"
source = "https://example.com"
modified = true

[overrides]
derivatives = "disallowed"
"#
    };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset FAILs as ModifiedUnderNoDerivatives.
    let codes = codes_for(&report, "statue");
    assert!(
        codes.contains(&FindingCode::ModifiedUnderNoDerivatives),
        "expected ModifiedUnderNoDerivatives, got {codes:?}"
    );
}

// Test case 10: derivatives="share-alike" (via override) → FLAG, not Fail.
#[test]
fn share_alike_is_flag_not_fail() {
    // Given a CC-BY asset overridden to share-alike, with licenses seeded.
    let tree = temptree! {
        "viral.glb": "binary",
        "viral.glb.attr.toml": r#"
title = "Viral"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com"

[overrides]
derivatives = "share-alike"
"#
    };
    let root = tree.path();
    seed_licenses(root);
    let svc = services();
    let cfg = non_commercial_config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset is FLAGged (ShareAlikeReview) but does not FAIL.
    let viral_fail = report
        .findings
        .iter()
        .any(|f| f.asset.to_string_lossy().contains("viral") && f.severity == Severity::Fail);
    assert!(!viral_fail, "share-alike must FLAG, not FAIL");
    let codes = codes_for(&report, "viral");
    assert!(
        codes.contains(&FindingCode::ShareAlikeReview),
        "expected ShareAlikeReview FLAG, got {codes:?}"
    );
    assert!(!report.has_failures());
}

// Test case 11: override flips allows_commercial_use — under a non-commercial
// project the asset still passes; confirms effective-terms computation flows.
#[test]
fn override_commercial_under_non_commercial_project_passes() {
    // Given a CC-BY asset overridden to non-commercial under a non-commercial project.
    let tree = temptree! {
        "ok.glb": "binary",
        "ok.glb.attr.toml": r#"
title = "Ok"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com"

[overrides]
allows_commercial_use = false
"#
    };
    let root = tree.path();
    seed_licenses(root);
    let svc = services();
    let cfg = non_commercial_config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then no failure is raised (non-commercial asset is fine under a non-commercial project).
    let codes = codes_for(&report, "ok.glb");
    assert!(
        !codes.contains(&FindingCode::NotCommerciallyLicensed),
        "non-commercial asset is fine under a non-commercial project; got {codes:?}"
    );
    assert!(!report.has_failures());
}

// Test case 12: asset excluded via [exclude] glob is not audited.
#[test]
fn excluded_glob_asset_not_audited() {
    // Given a project where vendor/** is excluded.
    let tree = temptree! {
        "vendor": {
            "skip.glb": "binary"
        }
    };
    let root = tree.path();
    let svc = services();
    let cfg = Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: vec!["vendor/**".to_string()],
    };
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the excluded asset does not appear in findings and there are no failures.
    assert!(
        report
            .findings
            .iter()
            .all(|f| !f.asset.to_string_lossy().contains("skip.glb")),
        "excluded asset should not appear in findings"
    );
    assert!(!report.has_failures());
}

// Test case 5: redistributes_assets=true + allows_redistribution=false -> FAIL.
#[test]
fn redistribution_violation_fails_when_project_redistributes() {
    // Given a redistributing project and an asset whose license forbids redistribution.
    let tree = temptree! {
        "sword.glb": "binary",
        "sword.glb.attr.toml": "title = \"Sword\"\nauthor = \"A\"\nyear = 2020\nlicense = \"CC-BY-3.0\"\nsource = \"https://x\"\n\n[overrides]\nallows_redistribution = false\n",
    };
    let root = tree.path();
    seed_licenses(root);
    let svc = services();
    let cfg = Config {
        commercial_project: false,
        redistributes_assets: true,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    };
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset FAILs as RedistributionViolation.
    let codes = codes_for(&report, "sword");
    assert!(
        codes.contains(&FindingCode::RedistributionViolation),
        "expected RedistributionViolation, got {codes:?}"
    );
}

// Test case 6: redistributes_assets=false + allows_redistribution=false -> clean.
#[test]
fn redistribution_gate_inactive_when_project_does_not_redistribute() {
    // Given a non-redistributing project and an asset whose license forbids redistribution.
    let tree = temptree! {
        "sword.glb": "binary",
        "sword.glb.attr.toml": "title = \"Sword\"\nauthor = \"A\"\nyear = 2020\nlicense = \"CC-BY-3.0\"\nsource = \"https://x\"\n\n[overrides]\nallows_redistribution = false\n",
    };
    let root = tree.path();
    seed_licenses(root);
    let svc = services();
    let cfg = Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    };
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then no redistribution finding fires (gate inactive) and audit is clean.
    assert!(
        report
            .findings
            .iter()
            .all(|f| f.code != FindingCode::RedistributionViolation),
        "redistribution gate should be inactive"
    );
    assert!(
        !report.has_failures(),
        "expected clean audit, got: {:?}",
        report.findings
    );
}

// Test case 7: manual_review=true, not acknowledged -> FAIL.
#[test]
fn manual_review_fails_until_acknowledged() {
    // Given a bespoke EULA license marked manual_review and an asset using it.
    let tree = temptree! {
        "fx.ogg": "binary",
        "fx.ogg.attr.toml": "title = \"FX\"\nauthor = \"A\"\nyear = 2020\nlicense = \"LicenseRef-EULA\"\nsource = \"https://x\"\n",
        "LICENSES": {
            "LicenseRef-EULA.txt": "custom eula text\n"
        },
        "licenses": {
            "LicenseRef-EULA.toml": r#"
id = "LicenseRef-EULA"
name = "Bespoke EULA"
url = "https://example.com/eula"
text = "custom eula text"
[terms]
requires_attribution = true
requires_license_notice = false
requires_source_disclosure = false
derivatives = "allowed"
requires_modification_notice = false
allows_commercial_use = true
allows_redistribution = false
manual_review = true
"#
        },
    };
    let root = tree.path();
    let fs = FsService::new(Arc::new(RealFs::new()));
    let registry = LicenseRegistry::load(&fs, root).unwrap();
    let svc = Services { fs, registry };
    let cfg = Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    };
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then the asset FAILs as ManualReviewRequired (not acknowledged).
    let codes = codes_for(&report, "fx");
    assert!(
        codes.contains(&FindingCode::ManualReviewRequired),
        "expected ManualReviewRequired, got {codes:?}"
    );
}

// Test case 8: manual_review=true, id in manual_review_acknowledged -> clean.
#[test]
fn manual_review_passes_when_acknowledged() {
    // Given a bespoke EULA license marked manual_review that IS acknowledged in config.
    let tree = temptree! {
        "fx.ogg": "binary",
        "fx.ogg.attr.toml": "title = \"FX\"\nauthor = \"A\"\nyear = 2020\nlicense = \"LicenseRef-EULA\"\nsource = \"https://x\"\n",
        "LICENSES": {
            "LicenseRef-EULA.txt": "custom eula text\n"
        },
        "licenses": {
            "LicenseRef-EULA.toml": r#"
id = "LicenseRef-EULA"
name = "Bespoke EULA"
url = "https://example.com/eula"
text = "custom eula text"
[terms]
requires_attribution = true
requires_license_notice = false
requires_source_disclosure = false
derivatives = "allowed"
requires_modification_notice = false
allows_commercial_use = true
allows_redistribution = false
manual_review = true
"#
        },
    };
    let root = tree.path();
    let fs = FsService::new(Arc::new(RealFs::new()));
    let registry = LicenseRegistry::load(&fs, root).unwrap();
    let svc = Services { fs, registry };
    let cfg = Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: vec!["LicenseRef-EULA".to_string()],
        exclude: Vec::new(),
    };
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).unwrap();

    // Then no manual-review finding fires and audit is clean.
    assert!(
        report
            .findings
            .iter()
            .all(|f| f.code != FindingCode::ManualReviewRequired),
        "acknowledged license should not surface manual-review finding"
    );
    assert!(
        !report.has_failures(),
        "expected clean audit, got: {:?}",
        report.findings
    );
}
