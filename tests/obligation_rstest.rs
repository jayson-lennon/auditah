//! rstest-parameterized obligation-check family: each case asserts that a
//! specific effective-term violation produces a specific finding code.
//!
//! All cases share the same property: "a violated obligation surfaces as its
//! exact `FindingCode`. Same assertion logic, different (setup, expected code).

use auditah::audit::report::FindingCode;
use auditah::audit::{run_audit, AuditCtx};
use auditah::config::Config;
use temptree::temptree;

mod common;
use common::{codes_for, commercial_config, non_commercial_config, services};

/// The shared assertion: the asset named by `needle` must surface `expected`.
fn assert_finding(ctx: &AuditCtx, needle: &str, expected: FindingCode) {
    let report = run_audit(ctx).expect("audit runs");
    let codes = codes_for(&report, needle);
    assert!(
        codes.contains(&expected),
        "expected {expected:?} for {needle:?}, got {codes:?}"
    );
}

// Same property: a violated obligation → its exact FindingCode.
#[rstest::rstest]
#[case::uncovered(
    "uncovered.glb",
    "",
    non_commercial_config(),
    FindingCode::UnlicensedAsset
)]
#[case::unknown_license(
    "unknown.glb",
    r#"
title = "U"
author = "A"
year = 2020
license = "GPL-3.0"
source = "https://example.com"
"#,
    non_commercial_config(),
    FindingCode::UnknownLicense
)]
#[case::incomplete_attribution(
    "incomplete.glb",
    r#"
title = "Incomplete"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = ""
"#,
    non_commercial_config(),
    FindingCode::IncompleteAttribution
)]
#[case::non_commercial_under_commercial(
    "nc.glb",
    r#"
title = "NC"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com"

[overrides]
allows_commercial_use = false
"#,
    commercial_config(),
    FindingCode::NotCommerciallyLicensed
)]
#[case::modified_under_no_derivatives(
    "mod.glb",
    r#"
title = "Mod"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com"
modified = true

[overrides]
allows_modifications = false
"#,
    non_commercial_config(),
    FindingCode::ModifiedUnderNoDerivatives
)]
fn obligation_violation_surfaces_expected_finding_code(
    #[case] asset_name: &str,
    #[case] sidecar: &str,
    #[case] config: Config,
    #[case] expected: FindingCode,
) {
    // Given an asset whose sidecar violates one obligation.
    let tree = temptree! {
        "asset.glb": "binary",
    };
    let root = tree.path();
    let asset_path = root.join(asset_name);
    std::fs::write(&asset_path, "binary").unwrap();
    if !sidecar.is_empty() {
        let sidecar_path = root.join(format!("{asset_name}.attr.toml"));
        std::fs::write(&sidecar_path, sidecar).unwrap();
    }
    let svc = services();
    let ctx = AuditCtx {
        services: &svc,
        config: &config,
        root,
    };

    // When running the audit and asserting the finding.
    // Then the violated obligation surfaces its exact FindingCode.
    assert_finding(&ctx, asset_name, expected);
}
