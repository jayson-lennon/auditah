//! Integration tests: the `generate` umbrella pipeline end-to-end.
//!
//! Asserts that `generate` writes all three artifacts (CREDITS.md, NOTICES.md,
//! BOM.md), audit-gates (aborts on FAIL without writing anything), and honors
//! custom `--output-*` paths.
//!
//! Tests the public contract via the command's `run()` function (the same entry
//! point `main.rs` calls). `run()` loads its own `Services` from disk, so
//! licenses must be seeded to `LICENSES/` on disk, not in-memory.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use auditah::cli::generate_cmd::{run, GenerateCmd};
use auditah::model::terms::LicenseTerms;
use auditah::registry::{LicenseRegistry, LicenseSpec};
use common::real_fs;
use temptree::temptree;

mod common;
use common::seed_license_text;

// --- term fixtures ---

fn cc0_terms() -> LicenseTerms {
    // CC0: no notice required.
    LicenseTerms::permissive()
}

fn mit_terms() -> LicenseTerms {
    // MIT: notice required.
    LicenseTerms {
        requires_license_notice: true,
        ..LicenseTerms::permissive()
    }
}

// --- helpers ---

/// Seed a license grid + text to `LICENSES/` on disk with the given terms.
fn seed_license_with_terms(root: &Path, id: &str, name: &str, terms: LicenseTerms) {
    LicenseRegistry::builder()
        .license(LicenseSpec::new(id).name(name).terms(terms))
        .commit(root, &real_fs())
        .expect("seed license commit");
    seed_license_text(root, &[id]);
}

const SIDECAR_WITH_LICENSE: &str = r#"
title = "Asset"
author = "Author"
year = 2024
license = "LicenseRef-Asset"
source = "https://example.com"
"#;

fn run_generate(
    root: &Path,
    out_credits: &Path,
    out_notices: &Path,
    out_bom: &Path,
) -> Result<auditah::cli::CommandStatus, error_stack::Report<auditah::AppError>> {
    let cmd = GenerateCmd {
        root: root.to_path_buf(),
        output_credits: Some(out_credits.to_path_buf()),
        output_notices: Some(out_notices.to_path_buf()),
        output_bom: Some(out_bom.to_path_buf()),
    };
    run(&cmd, root)
}

fn defaults(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        root.join("CREDITS.md"),
        root.join("NOTICES.md"),
        root.join("BOM.md"),
    )
}

// --- tests ---

#[test]
fn generate_writes_all_three_artifacts() {
    // Given a clean project with one MIT-like (notice-required) asset.
    let tree = temptree! {
        "asset.glb" : "",
        "asset.glb.attr.toml" : SIDECAR_WITH_LICENSE,
    };
    let root = tree.path();
    seed_license_with_terms(root, "LicenseRef-Asset", "Test Asset License", mit_terms());
    // When generate runs.
    let (c, n, b) = defaults(root);
    let result = run_generate(root, &c, &n, &b);

    // Then all three files exist.
    assert!(
        result.is_ok(),
        "generate should succeed: {:?}",
        result.err()
    );
    assert!(c.exists(), "CREDITS.md not written");
    assert!(n.exists(), "NOTICES.md not written");
    assert!(b.exists(), "BOM.md not written");
}

#[test]
fn generate_aborts_on_audit_failure_without_writing() {
    // Given a project with an asset whose license text is MISSING (audit FAILs).
    let tree = temptree! {
        "asset.glb" : "",
        "asset.glb.attr.toml" : SIDECAR_WITH_LICENSE,
    };
    let root = tree.path();
    // Seed the grid only (no .txt) → audit FAILs MissingLicenseText.
    LicenseRegistry::builder()
        .license(LicenseSpec::new("LicenseRef-Asset").terms(mit_terms()))
        .commit(root, &real_fs())
        .expect("seed grid");

    // When generate runs.
    let (c, n, b) = defaults(root);
    let result = run_generate(root, &c, &n, &b);

    // Then it errors and wrote nothing.
    assert!(result.is_err(), "generate should abort on audit FAIL");
    assert!(!c.exists(), "CREDITS.md should not be written on failure");
    assert!(!n.exists(), "NOTICES.md should not be written on failure");
    assert!(!b.exists(), "BOM.md should not be written on failure");
}

#[test]
fn generate_notices_reproduces_text_for_notice_required_license() {
    // Given a clean project with a notice-required asset.
    let tree = temptree! {
        "asset.glb" : "",
        "asset.glb.attr.toml" : SIDECAR_WITH_LICENSE,
    };
    let root = tree.path();
    seed_license_with_terms(root, "LicenseRef-Asset", "Test Asset License", mit_terms());

    // When generate runs.
    let (c, n, b) = defaults(root);
    run_generate(root, &c, &n, &b).expect("generate should succeed");

    // Then NOTICES.md contains the license text.
    let notices = std::fs::read_to_string(&n).expect("NOTICES.md readable");
    assert!(
        notices.contains("license body"),
        "NOTICES.md should contain license text:\n{notices}"
    );
    assert!(
        notices.contains("LicenseRef-Asset"),
        "NOTICES.md should contain the license id:\n{notices}"
    );
}

#[test]
fn generate_notices_omits_non_notice_licenses() {
    // Given a clean project with a CC0-like (no notice) asset.
    let tree = temptree! {
        "asset.glb" : "",
        "asset.glb.attr.toml" : SIDECAR_WITH_LICENSE,
    };
    let root = tree.path();
    seed_license_with_terms(root, "LicenseRef-Asset", "CC0", cc0_terms());

    // When generate runs.
    let (c, n, b) = defaults(root);
    run_generate(root, &c, &n, &b).expect("generate should succeed");

    // Then NOTICES.md has the empty placeholder (CC0 requires no notice).
    let notices = std::fs::read_to_string(&n).expect("NOTICES.md readable");
    assert!(
        notices.contains("_No license-notice-required assets found._"),
        "NOTICES.md should be empty for CC0-only project:\n{notices}"
    );
}

#[test]
fn generate_custom_output_paths() {
    // Given a clean project.
    let tree = temptree! {
        "asset.glb" : "",
        "asset.glb.attr.toml" : SIDECAR_WITH_LICENSE,
    };
    let root = tree.path();
    seed_license_with_terms(root, "LicenseRef-Asset", "Test Asset License", mit_terms());

    // When generate runs with custom output paths.
    let custom_credits = root.join("custom_credits.md");
    let custom_notices = root.join("custom_notices.md");
    let custom_bom = root.join("custom_bom.md");
    run_generate(root, &custom_credits, &custom_notices, &custom_bom)
        .expect("generate should succeed");

    // Then files are written to the custom paths, not the defaults.
    assert!(custom_credits.exists());
    assert!(custom_notices.exists());
    assert!(custom_bom.exists());
    assert!(!root.join("CREDITS.md").exists());
    assert!(!root.join("NOTICES.md").exists());
    assert!(!root.join("BOM.md").exists());
}

#[test]
fn generate_on_empty_project_writes_all_three() {
    // Given an empty project (no assets, no licenses) that has been initialized.
    let tree = temptree! {
        "auditah.toml": "",
        "LICENSES": {}, // init creates LICENSES/; discovery requires it
    };
    let root = tree.path();

    // When generate runs.
    let (c, n, b) = defaults(root);
    let result = run_generate(root, &c, &n, &b);

    // Then it succeeds and writes all three (with empty placeholders).
    assert!(
        result.is_ok(),
        "generate on empty project should succeed: {:?}",
        result.err()
    );
    assert!(c.exists());
    assert!(n.exists());
    assert!(b.exists());
}
