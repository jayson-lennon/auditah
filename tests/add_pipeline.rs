//! Integration tests: `add` and `init-pack` produce files that round-trip
//! through the audit pipeline cleanly.

use auditah::add::{render_record, write_manifest, write_sidecar};
use auditah::audit::{run_audit, AuditCtx};
use auditah::model::attribution::AttributionRecord;
use temptree::temptree;

mod common;
use common::{cfg, record, seed_licenses, services};

// `add` writes a sidecar that audit then accepts as covered.
#[test]
fn add_sidecar_makes_asset_pass_audit() {
    let tree = temptree! { "sword.glb": "binary" };
    let root = tree.path();
    seed_licenses(root);
    let svc = services();
    write_sidecar(&svc, &root.join("sword.glb"), &record("CC0-1.0")).unwrap();

    let ctx = AuditCtx {
        services: &svc,
        config: &cfg(),
        root,
    };
    let report = run_audit(&ctx).unwrap();
    assert!(
        !report.has_failures(),
        "sidecarred asset must pass; got {:?}",
        report.findings
    );
}

// `init-pack` writes a manifest that audit then accepts for every asset in the dir.
#[test]
fn init_pack_manifest_covers_entire_directory() {
    let tree = temptree! {
        "pack": {
            "rock.glb": "b",
            "tree.glb": "b",
            "bush.glb": "b",
        }
    };
    let root = tree.path();
    seed_licenses(root);
    let svc = services();
    write_manifest(&svc, &root.join("pack"), &record("CC0-1.0")).unwrap();

    let ctx = AuditCtx {
        services: &svc,
        config: &cfg(),
        root,
    };
    let report = run_audit(&ctx).unwrap();
    assert!(
        !report.has_failures(),
        "manifest-covered dir must pass; got {:?}",
        report.findings
    );
}

// `init-pack` covers nested subdirectories too (manifest applies to subtree).
#[test]
fn init_pack_manifest_covers_subdirectories() {
    let tree = temptree! {
        "pack": {
            "a.glb": "b",
            "sub": { "b.glb": "b" },
        }
    };
    let root = tree.path();
    seed_licenses(root);
    let svc = services();
    write_manifest(&svc, &root.join("pack"), &record("CC0-1.0")).unwrap();

    let ctx = AuditCtx {
        services: &svc,
        config: &cfg(),
        root,
    };
    let report = run_audit(&ctx).unwrap();
    assert!(
        !report.has_failures(),
        "manifest-covered subtree must pass; got {:?}",
        report.findings
    );
}

// A sidecar written by `add` overrides a manifest written by `init-pack`.
#[test]
fn add_sidecar_overrides_init_pack_manifest() {
    let tree = temptree! {
        "pack": {
            "special.glb": "b",
        }
    };
    let root = tree.path();
    seed_licenses(root);
    let svc = services();
    // Manifest says CC0; sidecar says CC-BY (which requires a non-empty source).
    write_manifest(&svc, &root.join("pack"), &record("CC0-1.0")).unwrap();
    let mut special = record("CC-BY-3.0");
    special.title = "Special".to_string();
    write_sidecar(&svc, &root.join("pack").join("special.glb"), &special).unwrap();

    let ctx = AuditCtx {
        services: &svc,
        config: &cfg(),
        root,
    };
    let report = run_audit(&ctx).unwrap();
    assert!(
        !report.has_failures(),
        "sidecar override must win; got {:?}",
        report.findings
    );
}

// `render_record` output parses back to the same record (idempotent scaffold).
#[test]
fn render_record_round_trips_into_audit_record() {
    let rec = record("CC-BY-3.0");
    let toml = render_record(&rec);
    let parsed: AttributionRecord = toml::from_str(&toml).expect("round-trip");
    assert_eq!(parsed, rec);
}
