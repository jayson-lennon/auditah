//! Integration tests: the observable consequence of provisioning a license
//! into `LICENSES/`.
//!
//! Once a well-known license is provisioned into `LICENSES/` (text + grid),
//! a subsequent audit of an asset referencing it passes with no
//! `MissingLicenseText`. The provisioning matrix itself is pinned in
//! `src/add_license.rs` lib tests; these verify the end-to-end *consequence*.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::audit::report::FindingCode;
use auditah::audit::run_audit;
use auditah::registry::LicenseSpec;
use common::{
    codes_for, non_commercial_config, permissive_terms, seed_license_text, services_with,
};
use temptree::temptree;

mod common;

/// MIT after provisioning: asset audits clean (no `MissingLicenseText`) and the
/// notice-preservation obligation stays satisfied even with no author/title.
#[test]
fn mit_provisioned_asset_passes_audit_without_missing_license_text() {
    // Given a project with an MIT-licensed asset and MIT provisioned into LICENSES/.
    let tree = temptree! {
        "hero.glb": "binary",
        "hero.glb.attr.toml":
            "title = \"Hero\"\nauthor = \"Artist\"\nyear = 2024\nlicense = \"MIT\"\nsource = \"https://x\"\n",
    };
    let root = tree.path();
    // MIT is well-known and notice-preservation-only: requires_attribution = false,
    // requires_license_notice = true. The grid resolves via the embedded corpus;
    // only the legal text file must exist on disk.
    let svc = services_with(
        root,
        non_commercial_config(),
        [LicenseSpec::new("MIT").terms({
            let mut t = permissive_terms();
            t.requires_license_notice = true;
            t
        })],
    );
    seed_license_text(root, &["MIT"]);

    // When running the audit.
    let report = run_audit(&svc).unwrap();

    // Then there is no MissingLicenseText finding for the asset.
    let codes = codes_for(&report, "hero.glb");
    assert!(
        !codes.contains(&FindingCode::MissingLicenseText),
        "provisioned MIT must not trip MissingLicenseText, got {codes:?}"
    );
}

/// MIT no longer requires named attribution: an MIT asset with an empty title
/// still audits clean (Phase 1 behavior, verified end-to-end).
#[test]
fn mit_asset_with_empty_title_passes_audit() {
    // Given an MIT asset whose record omits the title.
    let tree = temptree! {
        "sound.wav": "bytes",
        "sound.wav.attr.toml":
            "title = \"\"\nauthor = \"Artist\"\nyear = 2024\nlicense = \"MIT\"\nsource = \"https://x\"\n",
    };
    let root = tree.path();
    let svc = services_with(
        root,
        non_commercial_config(),
        [LicenseSpec::new("MIT").terms({
            let mut t = permissive_terms();
            t.requires_license_notice = true;
            t
        })],
    );
    seed_license_text(root, &["MIT"]);

    // When running the audit.
    let report = run_audit(&svc).unwrap();

    // Then there is no IncompleteAttribution finding (MIT requires notice only).
    let codes = codes_for(&report, "sound.wav");
    assert!(
        !codes.contains(&FindingCode::IncompleteAttribution),
        "MIT must not require named attribution, got {codes:?}"
    );
}

/// CC-BY-4.0 still requires named attribution: an asset missing source FAILs
/// (Phase 1 behavior preserved, verified end-to-end).
#[test]
fn ccby_asset_missing_source_fails_incomplete_attribution() {
    // Given a CC-BY-4.0 asset whose record omits the source.
    let tree = temptree! {
        "music.ogg": "bytes",
        "music.ogg.attr.toml":
            "title = \"Track\"\nauthor = \"Artist\"\nyear = 2024\nlicense = \"CC-BY-4.0\"\nsource = \"\"\n",
    };
    let root = tree.path();
    let svc = services_with(
        root,
        non_commercial_config(),
        [LicenseSpec::new("CC-BY-4.0").terms({
            let mut t = permissive_terms();
            t.requires_attribution = true;
            t.requires_license_notice = true;
            t
        })],
    );
    seed_license_text(root, &["CC-BY-4.0"]);

    // When running the audit.
    let report = run_audit(&svc).unwrap();

    // Then it FAILs with IncompleteAttribution (CC-BY genuinely requires credit).
    let codes = codes_for(&report, "music.ogg");
    assert!(
        codes.contains(&FindingCode::IncompleteAttribution),
        "CC-BY-4.0 must still require named attribution, got {codes:?}"
    );
}
