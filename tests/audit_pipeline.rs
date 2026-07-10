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

/// Build a real-filesystem Services container with the embedded license
/// registry (no project-local licenses in these scenarios).
fn services() -> Services {
    Services {
        fs: FsService::new(Arc::new(RealFs::new())),
        registry: LicenseRegistry::embedded_only(),
    }
}

/// Seed `LICENSES/<id>.txt` for every embedded license so audit's
/// MissingLicenseText check passes in these pass-clean scenarios.
fn seed_licenses(root: &std::path::Path) {
    let reg = LicenseRegistry::embedded_only();
    let dir = root.join("LICENSES");
    std::fs::create_dir_all(&dir).unwrap();
    for entry in reg.entries() {
        std::fs::write(dir.join(format!("{}.txt", entry.id)), &entry.text).unwrap();
    }
}

fn non_commercial_config() -> Config {
    Config {
        commercial_project: false,
        exclude: Vec::new(),
    }
}

/// Collect the finding codes for assets whose name contains `needle`.
fn codes_for(report: &auditah::audit::report::AuditReport, needle: &str) -> Vec<FindingCode> {
    report
        .findings
        .iter()
        .filter(|f| f.asset.to_string_lossy().contains(needle))
        .map(|f| f.code)
        .collect()
}

// Test case 1: uncovered asset → FAIL UnlicensedAsset.
#[test]
fn uncovered_asset_fails_as_unlicensed() {
    let tree = temptree! { "sword.glb": "binary" };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };
    let report = run_audit(&ctx).unwrap();
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
    let report = run_audit(&ctx).unwrap();
    let codes = codes_for(&report, "ghost");
    assert!(
        codes.contains(&FindingCode::OrphanSidecar),
        "expected OrphanSidecar, got {codes:?}"
    );
}

// Test case 6: unknown license id → FAIL UnknownLicense.
#[test]
fn unknown_license_id_fails() {
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
    let report = run_audit(&ctx).unwrap();
    let codes = codes_for(&report, "rock.glb");
    assert!(
        codes.contains(&FindingCode::UnknownLicense),
        "expected UnknownLicense, got {codes:?}"
    );
}

// Test case 7: requires_attribution + missing source → FAIL IncompleteAttribution.
#[test]
fn incomplete_attribution_missing_source_fails() {
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
    let report = run_audit(&ctx).unwrap();
    let codes = codes_for(&report, "tile.glb");
    assert!(
        codes.contains(&FindingCode::IncompleteAttribution),
        "expected IncompleteAttribution, got {codes:?}"
    );
}

// Test case 8: allows_commercial_use=false (via override) + commercial_project → FAIL.
#[test]
fn non_commercial_asset_fails_under_commercial_project() {
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
        exclude: Vec::new(),
    };
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };
    let report = run_audit(&ctx).unwrap();
    let codes = codes_for(&report, "fanfare");
    assert!(
        codes.contains(&FindingCode::NotCommerciallyLicensed),
        "expected NotCommerciallyLicensed, got {codes:?}"
    );
}

// Test case 9: allows_modifications=false (via override) + modified=true → FAIL.
#[test]
fn modified_under_no_derivatives_fails() {
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
allows_modifications = false
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
    let report = run_audit(&ctx).unwrap();
    let codes = codes_for(&report, "statue");
    assert!(
        codes.contains(&FindingCode::ModifiedUnderNoDerivatives),
        "expected ModifiedUnderNoDerivatives, got {codes:?}"
    );
}

// Test case 10: requires_share_alike=true (via override) → FLAG, not Fail.
#[test]
fn share_alike_is_flag_not_fail() {
    let tree = temptree! {
        "viral.glb": "binary",
        "viral.glb.attr.toml": r#"
title = "Viral"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com"

[overrides]
requires_share_alike = true
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
    let report = run_audit(&ctx).unwrap();
    // No blocking failures for this asset.
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
    let report = run_audit(&ctx).unwrap();
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
    let tree = temptree! {
        "vendor": {
            "skip.glb": "binary"
        }
    };
    let root = tree.path();
    let svc = services();
    let cfg = Config {
        commercial_project: false,
        exclude: vec!["vendor/**".to_string()],
    };
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg,
        root,
    };
    let report = run_audit(&ctx).unwrap();
    // skip.glb is excluded → no findings about it, and no failures at all.
    assert!(
        report
            .findings
            .iter()
            .all(|f| !f.asset.to_string_lossy().contains("skip.glb")),
        "excluded asset should not appear in findings"
    );
    assert!(!report.has_failures());
}
