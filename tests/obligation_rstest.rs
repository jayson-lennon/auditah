//! rstest-parameterized obligation-check family: each case asserts that a
//! specific effective-term violation produces a specific finding code.
//!
//! All cases share the same property: "a violated obligation surfaces as its
//! exact `FindingCode`. Same assertion logic, different (setup, expected code).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::audit::report::FindingCode;
use auditah::audit::{run_audit, AuditCtx};
use auditah::config::Config;
use temptree::temptree;

mod common;
use auditah::model::terms::LicenseTerms;
use auditah::registry::LicenseSpec;
use common::{
    codes_for, commercial_config, non_commercial_config, permissive_terms, services_with,
};

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
license = "LicenseRef-Unknown"
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
license = "LicenseRef-CcBy"
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
license = "LicenseRef-CcBy"
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
license = "LicenseRef-CcBy"
source = "https://example.com"
modified = true

[overrides]
derivatives = "disallowed"
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
    // A permissive-with-attribution license covers the attribution-requiring cases;
    // the unknown/uncovered cases never resolve against it.
    let svc = services_with([LicenseSpec::new("LicenseRef-CcBy").terms(LicenseTerms {
        requires_attribution: true,
        ..permissive_terms()
    })]);
    let ctx = AuditCtx {
        services: &svc,
        config: &config,
        root,
    };

    // When running the audit.
    let report = run_audit(&ctx).expect("audit runs");

    // Then the violated obligation surfaces its exact FindingCode.
    let codes = codes_for(&report, asset_name);
    assert!(
        codes.contains(&expected),
        "expected {expected:?} for {asset_name:?}, got {codes:?}"
    );
}
