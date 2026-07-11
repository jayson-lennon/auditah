//! Integration tests: the `bom` pipeline end-to-end against a real temp fs.
//! Asserts on the generated BOM.md content (the public contract).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use auditah::bom::{generate_bom, BomCtx};
use auditah::config::Config;
use auditah::model::terms::{Derivatives, LicenseTerms};
use auditah::registry::LicenseSpec;
use auditah::services::Services;
use temptree::temptree;

mod common;
use common::{config, services_with};

// --- term fixtures ---

fn notice_terms() -> LicenseTerms {
    LicenseTerms {
        requires_license_notice: true,
        ..LicenseTerms::permissive()
    }
}

fn source_disclosure_terms() -> LicenseTerms {
    LicenseTerms {
        requires_source_disclosure: true,
        ..LicenseTerms::permissive()
    }
}

fn share_alike_terms() -> LicenseTerms {
    LicenseTerms::permissive().with_derivatives(Derivatives::ShareAlike)
}

// --- helpers ---

/// Generate BOM to `<root>/BOM.md` and return the file contents.
///
/// Seeds `LICENSES/<id>.txt` for each `ids` entry so the audit gate passes.
fn generated(ctx: &BomCtx, ids: &[&str]) -> String {
    if !ids.is_empty() {
        common::seed_license_text(ctx.root, ids);
    }
    let out = ctx.root.join("BOM.md");
    generate_bom(ctx, &out).expect("BOM generation should succeed");
    std::fs::read_to_string(&out).expect("BOM.md should be readable")
}

fn ctx<'a>(svc: &'a Services, cfg: &'a Config, root: &'a Path) -> BomCtx<'a> {
    BomCtx {
        services: svc,
        config: cfg,
        root,
    }
}

const SIDECAR_A: &str = r#"
title = "Alpha"
author = "Alice"
year = 2022
license = "LicenseRef-Mit"
source = "https://a"
"#;

const SIDECAR_B: &str = r#"
title = "Beta"
author = "Bob"
year = 2022
license = "LicenseRef-Mit"
source = "https://b"
"#;

const SIDECAR_C_CC0: &str = r#"
title = "Gamma"
author = "Carol"
year = 2022
license = "LicenseRef-Cc0"
source = "https://c"
"#;

// --- tests ---

// Permissive + CC0 assets both appear in the BOM summary (unlike credits,
// which omits CC0). Asset counts are correct.
#[test]
fn permissive_and_cc0_both_appear_in_summary_with_counts() {
    // Given two MIT assets and one CC0 asset.
    let tree = temptree! {
        "a.glb": "binary",
        "a.glb.attr.toml": SIDECAR_A,
        "b.glb": "binary",
        "b.glb.attr.toml": SIDECAR_B,
        "c.glb": "binary",
        "c.glb.attr.toml": SIDECAR_C_CC0,
    };
    let root = tree.path();
    let svc = services_with([
        LicenseSpec::new("LicenseRef-Mit").name("MIT"),
        LicenseSpec::new("LicenseRef-Cc0").name("CC0"),
    ]);
    let cfg = config();

    // When generating the BOM.
    let bom = generated(
        &ctx(&svc, &cfg, root),
        &["LicenseRef-Mit", "LicenseRef-Cc0"],
    );

    // Then both licenses appear with correct asset counts, and CC0 is NOT omitted.
    assert!(
        bom.contains("LicenseRef-Mit") && bom.contains("2 asset"),
        "MIT with 2 assets missing:\n{bom}"
    );
    assert!(
        bom.contains("LicenseRef-Cc0") && bom.contains("1 asset"),
        "CC0 with 1 asset missing:\n{bom}"
    );
}

// A GPL asset (source disclosure) produces a source-offering action item with its path.
#[test]
fn source_disclosure_produces_action_item_with_path() {
    // Given a GPL asset requiring source disclosure.
    let tree = temptree! {
        "lib.glb": "binary",
        "lib.glb.attr.toml": r#"
title = "Lib"
author = "Gpl"
year = 2022
license = "LicenseRef-Gpl"
source = "https://g"
"#,
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-Gpl")
        .name("GPL")
        .terms(source_disclosure_terms())]);
    let cfg = config();

    // When generating the BOM.
    let bom = generated(&ctx(&svc, &cfg, root), &["LicenseRef-Gpl"]);

    // Then the action-items section mentions source disclosure and the asset path.
    assert!(
        bom.contains("source") && bom.contains("lib.glb"),
        "source disclosure action item missing:\n{bom}"
    );
}

// A CC-BY asset (license notice) produces a notice action item pointing at NOTICES.md.
#[test]
fn license_notice_produces_action_item_referencing_notices() {
    // Given a CC-BY asset requiring a license notice.
    let tree = temptree! {
        "font.ttf": "binary",
        "font.ttf.attr.toml": r#"
title = "Font"
author = "FontMaker"
year = 2022
license = "LicenseRef-CcBy"
source = "https://f"
"#,
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-CcBy")
        .name("CC-BY")
        .terms(notice_terms())]);
    let cfg = config();

    // When generating the BOM.
    let bom = generated(&ctx(&svc, &cfg, root), &["LicenseRef-CcBy"]);

    // Then the action-items section references NOTICES.md.
    assert!(
        bom.contains("NOTICES.md"),
        "license notice → NOTICES.md reference missing:\n{bom}"
    );
}

// A single share-alike license produces an SA action item but no conflict warning.
#[test]
fn single_share_alike_produces_obligation_no_conflict_warning() {
    // Given one CC-BY-SA asset.
    let tree = temptree! {
        "mesh.glb": "binary",
        "mesh.glb.attr.toml": r#"
title = "Mesh"
author = "Maker"
year = 2022
license = "LicenseRef-CcBySa"
source = "https://m"
"#,
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-CcBySa")
        .name("CC-BY-SA")
        .terms(share_alike_terms())]);
    let cfg = config();

    // When generating the BOM.
    let bom = generated(&ctx(&svc, &cfg, root), &["LicenseRef-CcBySa"]);

    assert!(
        bom.contains("must ship under"),
        "share-alike action item text missing:\n{bom}"
    );
    assert!(
        bom.contains("LicenseRef-CcBySa"),
        "SA license summary missing:\n{bom}"
    );
    assert!(
        !bom.contains("Multiple share-alike"),
        "unexpected conflict warning for single SA:\n{bom}"
    );
}

// Two distinct share-alike licenses produce a conflict warning.
#[test]
fn two_distinct_share_alike_licenses_produce_conflict_warning() {
    // Given one CC-BY-SA asset and one GPL asset, both share-alike.
    let tree = temptree! {
        "a.glb": "binary",
        "a.glb.attr.toml": r#"
title = "Alpha"
author = "Alice"
year = 2022
license = "LicenseRef-CcBySa"
source = "https://a"
"#,
        "b.glb": "binary",
        "b.glb.attr.toml": r#"
title = "Beta"
author = "Bob"
year = 2022
license = "LicenseRef-Gpl"
source = "https://b"
"#,
    };
    let root = tree.path();
    let svc = services_with([
        LicenseSpec::new("LicenseRef-CcBySa")
            .name("CC-BY-SA")
            .terms(share_alike_terms()),
        LicenseSpec::new("LicenseRef-Gpl")
            .name("GPL")
            .terms(share_alike_terms()),
    ]);
    let cfg = config();

    // When generating the BOM.
    let bom = generated(
        &ctx(&svc, &cfg, root),
        &["LicenseRef-CcBySa", "LicenseRef-Gpl"],
    );

    // Then a conflict warning names both licenses.
    assert!(
        bom.contains("Multiple share-alike"),
        "conflict warning missing:\n{bom}"
    );
    assert!(
        bom.contains("LicenseRef-CcBySa") && bom.contains("LicenseRef-Gpl"),
        "both SA license ids should appear in the warning:\n{bom}"
    );
}

// An all-permissive project produces a summary but no action items.
#[test]
fn all_permissive_project_has_summary_but_no_action_items() {
    // Given two MIT assets (no obligations).
    let tree = temptree! {
        "a.glb": "binary",
        "a.glb.attr.toml": SIDECAR_A,
        "b.glb": "binary",
        "b.glb.attr.toml": SIDECAR_B,
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-Mit").name("MIT")]);
    let cfg = config();

    // When generating the BOM.
    let bom = generated(&ctx(&svc, &cfg, root), &["LicenseRef-Mit"]);

    // Then the summary is present but action items note nothing outstanding.
    assert!(bom.contains("LicenseRef-Mit"), "summary missing:\n{bom}");
    assert!(
        bom.contains("No outstanding compliance actions"),
        "expected empty action-items note:\n{bom}"
    );
}

// An empty project (no assets) produces an empty BOM that's still written.
#[test]
fn empty_project_writes_bom_with_no_licensed_assets() {
    // Given a project with no assets.
    let tree = temptree! {};
    // (no assets; registry has a license but nothing references it)
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-Mit")]);
    let cfg = config();

    // When generating the BOM.
    let bom = generated(&ctx(&svc, &cfg, root), &["LicenseRef-Mit"]);

    // Then the BOM notes no licensed assets were found.
    assert!(
        bom.contains("No licensed assets found"),
        "expected empty-project note:\n{bom}"
    );
}

// The --output flag writes to a custom path.
#[test]
fn custom_output_path_writes_to_specified_file() {
    // Given a project with one asset.
    let tree = temptree! {
        "a.glb": "binary",
        "a.glb.attr.toml": SIDECAR_A,
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-Mit")]);
    let cfg = config();
    let custom = root.join("custom-bom.md");

    // When generating to a custom path (seed license text so audit gate passes).
    common::seed_license_text(root, &["LicenseRef-Mit"]);
    generate_bom(&ctx(&svc, &cfg, root), &custom).expect("BOM generation should succeed");

    // Then the custom file exists and BOM.md does not.
    assert!(custom.exists(), "custom output not written");
    assert!(
        !root.join("BOM.md").exists(),
        "default BOM.md should not exist when custom path given"
    );
}

// The BOM respects exclude globs: excluded assets don't appear.
#[test]
fn excluded_assets_do_not_appear_in_bom() {
    // Given two assets, one excluded via config.
    let tree = temptree! {
        "keep.glb": "binary",
        "keep.glb.attr.toml": r#"
title = "Keep"
author = "Alice"
year = 2022
license = "LicenseRef-Mit"
source = "https://k"
"#,
        "skip.glb": "binary",
        "skip.glb.attr.toml": r#"
title = "Skip"
author = "Bob"
year = 2022
license = "LicenseRef-Mit"
source = "https://s"
"#,
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-Mit")]);
    let mut cfg = config();
    cfg.exclude = vec!["**/skip.glb".to_string()];
    // When generating the BOM.
    let bom = generated(&ctx(&svc, &cfg, root), &["LicenseRef-Mit"]);

    // Then only the kept asset appears (count = 1, not 2).
    assert!(
        bom.contains("1 asset"),
        "expected 1 asset after exclusion; got:\n{bom}"
    );
    assert!(
        !bom.contains("Skip"),
        "excluded asset leaked into BOM:\n{bom}"
    );
}

// A project with multiple license types produces a BOM with all summaries and
// ordered action items (conflict warnings, then per-license items in id order).
// Shared builder for the 3-asset multi-obligation project: MIT (permissive),
// CC-BY (notice), GPL (source disclosure). Reads the BOM inside so the temptree
// outlives the read.
fn multi_obligation_bom() -> String {
    let tree = temptree! {
        "mit.glb": "binary",
        "mit.glb.attr.toml": r#"
title = "Mit Asset"
author = "Alice"
year = 2022
license = "LicenseRef-Mit"
source = "https://m"
"#,
        "ccby.glb": "binary",
        "ccby.glb.attr.toml": r#"
title = "CC-BY Asset"
author = "Bob"
year = 2022
license = "LicenseRef-CcBy"
source = "https://c"
"#,
        "gpl.glb": "binary",
        "gpl.glb.attr.toml": r#"
title = "GPL Asset"
author = "Carol"
year = 2022
license = "LicenseRef-Gpl"
source = "https://g"
"#,
    };
    let root = tree.path();
    let svc = services_with([
        LicenseSpec::new("LicenseRef-Mit").name("MIT"),
        LicenseSpec::new("LicenseRef-CcBy")
            .name("CC-BY")
            .terms(notice_terms()),
        LicenseSpec::new("LicenseRef-Gpl")
            .name("GPL")
            .terms(source_disclosure_terms()),
    ]);
    let cfg = config();
    generated(
        &ctx(&svc, &cfg, root),
        &["LicenseRef-Mit", "LicenseRef-CcBy", "LicenseRef-Gpl"],
    )
}

#[test]
fn bom_summaries_include_all_license_types() {
    // Given a project with MIT, CC-BY, and GPL assets.
    // When generating the BOM.
    let bom = multi_obligation_bom();

    // Then all 3 licenses appear in the summary.
    assert!(
        bom.contains("LicenseRef-Mit"),
        "MIT summary missing:\n{bom}"
    );
    assert!(
        bom.contains("LicenseRef-CcBy"),
        "CC-BY summary missing:\n{bom}"
    );
    assert!(
        bom.contains("LicenseRef-Gpl"),
        "GPL summary missing:\n{bom}"
    );
}

#[test]
fn bom_action_items_reference_notice_and_source_obligations() {
    // Given a project with MIT, CC-BY, and GPL assets.
    // When generating the BOM.
    let bom = multi_obligation_bom();

    // Then action items reference both the notice and source obligations.
    assert!(
        bom.contains("NOTICES.md"),
        "notice action item missing:\n{bom}"
    );
    assert!(
        bom.contains("Offer corresponding source"),
        "source disclosure action item missing:\n{bom}"
    );
    assert!(
        bom.contains("gpl.glb"),
        "GPL asset path missing from action items:\n{bom}"
    );
}

#[test]
fn bom_has_no_share_alike_conflict_warning() {
    // Given a project with no share-alike licenses.
    // When generating the BOM.
    let bom = multi_obligation_bom();

    // Then there is no share-alike conflict warning.
    assert!(!bom.contains("Multiple share-alike"));
}
