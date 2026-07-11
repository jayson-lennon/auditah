//! Integration tests: `add` and `init-pack` produce files that round-trip
//! through the audit pipeline cleanly.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::add::{render_record, write_manifest, write_sidecar};
use auditah::audit::{run_audit, AuditCtx};
use auditah::model::attribution::AttributionRecord;
use temptree::temptree;

mod common;
use auditah::registry::LicenseSpec;
use common::{cfg, record, seed_license_text, services_with};

// `add` writes a sidecar that audit then accepts as covered.
#[test]
fn add_sidecar_makes_asset_pass_audit() {
    // Given an uncovered asset and a CC0 sidecar written by `add`.
    let tree = temptree! { "sword.glb": "binary" };
    let root = tree.path();
    seed_license_text(root, &["LicenseRef-Asset"]);
    let svc = services_with([LicenseSpec::new("LicenseRef-Asset")]);
    write_sidecar(&svc, &root.join("sword.glb"), &record("LicenseRef-Asset")).unwrap();

    // When running the audit.
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg(),
        root,
    };
    let report = run_audit(&ctx).unwrap();

    // Then the sidecarred asset passes (no failures).
    assert!(
        !report.has_failures(),
        "sidecarred asset must pass; got {:?}",
        report.findings
    );
}

// `init-pack` writes a manifest that audit then accepts for every asset in the dir.
#[test]
fn init_pack_manifest_covers_entire_directory() {
    // Given a pack directory with three uncovered assets and a CC0 manifest.
    let tree = temptree! {
        "pack": {
            "rock.glb": "b",
            "tree.glb": "b",
            "bush.glb": "b",
        }
    };
    let root = tree.path();
    seed_license_text(root, &["LicenseRef-Asset"]);
    let svc = services_with([LicenseSpec::new("LicenseRef-Asset")]);
    write_manifest(&svc, &root.join("pack"), &record("LicenseRef-Asset")).unwrap();

    // When running the audit.
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg(),
        root,
    };
    let report = run_audit(&ctx).unwrap();

    // Then every asset in the dir passes (manifest covers all).
    assert!(
        !report.has_failures(),
        "manifest-covered dir must pass; got {:?}",
        report.findings
    );
}

// `init-pack` covers nested subdirectories too (manifest applies to subtree).
#[test]
fn init_pack_manifest_covers_subdirectories() {
    // Given a pack directory with a nested subdirectory, covered by one manifest.
    let tree = temptree! {
        "pack": {
            "a.glb": "b",
            "sub": { "b.glb": "b" },
        }
    };
    let root = tree.path();
    seed_license_text(root, &["LicenseRef-Asset"]);
    let svc = services_with([LicenseSpec::new("LicenseRef-Asset")]);
    write_manifest(&svc, &root.join("pack"), &record("LicenseRef-Asset")).unwrap();

    // When running the audit.
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg(),
        root,
    };
    let report = run_audit(&ctx).unwrap();

    // Then the nested asset also passes (manifest applies to subtree).
    assert!(
        !report.has_failures(),
        "manifest-covered subtree must pass; got {:?}",
        report.findings
    );
}

// A sidecar written by `add` overrides a manifest written by `init-pack`.
#[test]
fn add_sidecar_overrides_init_pack_manifest() {
    // Given a pack with a CC0 manifest and a per-file CC-BY sidecar on one asset.
    let tree = temptree! {
        "pack": {
            "special.glb": "b",
        }
    };
    let root = tree.path();
    seed_license_text(root, &["LicenseRef-Asset", "LicenseRef-Custom"]);
    let svc = services_with([
        LicenseSpec::new("LicenseRef-Asset"),
        LicenseSpec::new("LicenseRef-Custom"),
    ]);
    write_manifest(&svc, &root.join("pack"), &record("LicenseRef-Asset")).unwrap();
    let mut special = record("LicenseRef-Custom");
    special.title = "Special".to_string();
    write_sidecar(&svc, &root.join("pack").join("special.glb"), &special).unwrap();

    // When running the audit.
    let ctx = AuditCtx {
        services: &svc,
        config: &cfg(),
        root,
    };
    let report = run_audit(&ctx).unwrap();

    // Then the sidecar wins and the asset passes.
    assert!(
        !report.has_failures(),
        "sidecar override must win; got {:?}",
        report.findings
    );
}

// `render_record` output parses back to the same record (idempotent scaffold).
#[test]
fn render_record_round_trips_into_audit_record() {
    // Given a CC-BY attribution record.
    let rec = record("LicenseRef-Custom");

    // When rendering to TOML and parsing back.
    let toml = render_record(&rec);
    let parsed: AttributionRecord = toml::from_str(&toml).expect("round-trip");

    // Then the parsed record equals the original.
    assert_eq!(parsed, rec);
}

// `init-pack` writes the manifest to the `_manifest.toml` filename.
#[test]
fn init_pack_writes_underscore_manifest() {
    // Given a pack directory.
    let tree = temptree! {
        "pack": { "rock.glb": "b" }
    };
    let root = tree.path();
    seed_license_text(root, &["LicenseRef-Asset"]);
    let svc = services_with([LicenseSpec::new("LicenseRef-Asset")]);
    let pack = root.join("pack");

    // When writing a manifest for the pack.
    write_manifest(&svc, &pack, &record("LicenseRef-Asset")).unwrap();

    // Then the manifest is written as `_manifest.toml`, not the legacy name.
    assert!(svc.fs.exists(&pack.join("_manifest.toml")));
    assert!(!svc.fs.exists(&pack.join("manifest.toml")));
}
