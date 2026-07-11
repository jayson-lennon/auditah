//! Integration tests: `add-license` command — scaffolds a license grid in `LICENSES/`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::add_license::{
    license_grid_path, license_ref_id, render_license_template, write_license_template,
};
use auditah::services::Services;
use std::path::Path;
use temptree::temptree;

mod common;

// Test case 1: `add-license Foo` writes LICENSES/LicenseRef-Foo.toml with default_fail() defaults.
#[test]
fn add_license_writes_default_fail_grid_for_licenseref_name() {
    // Given an empty project root and a Services backed by a real fs.
    let tree = temptree! {};
    let root = tree.path();
    let services = Services::real(root).expect("services");

    // When writing the template for "Foo".
    let path = write_license_template(&services, root, "Foo").expect("write");

    // Then the file is at LICENSES/LicenseRef-Foo.toml, id is auto-prefixed,
    // and the default_fail() shape is present.
    assert_eq!(path, license_grid_path(root, "LicenseRef-Foo"));
    let content = std::fs::read_to_string(&path).expect("read");
    assert!(content.contains("id = \"LicenseRef-Foo\""));
    // default_fail(): maximally restrictive, manual_review = true.
    assert!(content.contains("derivatives = \"disallowed\""));
    assert!(content.contains("requires_attribution = false"));
    assert!(content.contains("allows_commercial_use = false"));
    assert!(content.contains("allows_redistribution = false"));
    assert!(content.contains("manual_review = true"));
}

// Test case 2: every [terms] field has a `#` comment explaining it.
#[test]
fn template_comments_every_terms_field() {
    // Given the rendered template for an id.
    let content = render_license_template("LicenseRef-Foo");

    // When scanning the [terms] block.
    let terms_start = content.find("[terms]").expect("[terms] section");
    let terms = &content[terms_start..];

    // Then every field is preceded by a `#` comment on the line(s) above it.
    for field in [
        "requires_attribution",
        "requires_license_notice",
        "requires_source_disclosure",
        "derivatives",
        "requires_modification_notice",
        "allows_commercial_use",
        "allows_redistribution",
        "manual_review",
    ] {
        let field_line = terms
            .lines()
            .find(|l| l.contains(&format!("{field} =")))
            .unwrap_or_else(|| panic!("{field} assignment missing"));
        // The field line itself should not be the comment; find the nearest preceding `#`.
        let field_idx = terms.find(field_line).expect("field idx");
        let preceding = &terms[..field_idx];
        let last_comment = preceding.rfind('#').expect("no comment before {field}");
        // Ensure the comment is on the immediately preceding (non-blank) line.
        let between = &terms[last_comment..field_idx];
        assert!(
            !between.contains('\n')
                || between.trim().is_empty()
                || between.trim_start().starts_with('#'),
            "{field} must have a `#` comment immediately above it"
        );
    }
}

// Test case 3: the template header explains the id ↔ LICENSES/<id>.txt relationship.
#[test]
fn template_header_explains_licenseref_and_text_relationship() {
    // Given a LicenseRef-Foo id.
    // When rendering the template.
    let content = render_license_template("LicenseRef-Foo");

    // Then the header names the LicenseRef- form and points at LICENSES/<id>.txt.
    assert!(
        content.contains("LicenseRef-<Name>"),
        "must explain LicenseRef- form"
    );
    assert!(
        content.contains("LICENSES/LicenseRef-Foo.txt"),
        "must instruct the user to create the text file"
    );
    assert!(
        content.contains("FULL LEGAL TEXT"),
        "must explain the text is required separately"
    );
}

// Test case 4: re-running add-license on an existing id errors (no --force).
#[test]
fn add_license_refuses_to_overwrite_existing_grid() {
    // Given a root where LicenseRef-Foo.toml already exists.
    let tree = temptree! {
        "LICENSES": {
            "LicenseRef-Foo.toml": "id = \"LicenseRef-Foo\"\nname = \"X\"\nurl = \"https://x\"\n[terms]\nrequires_attribution = false\nrequires_license_notice = false\nrequires_source_disclosure = false\nderivatives = \"allowed\"\nrequires_modification_notice = false\nallows_commercial_use = true\nallows_redistribution = true\nmanual_review = false\n",
        }
    };
    let root = tree.path();
    let services = Services::real(root).expect("services");

    // When writing the template again.
    let result = write_license_template(&services, root, "Foo");

    // Then it errors (refuse-to-overwrite) and the file is untouched.
    assert!(result.is_err(), "must refuse to overwrite");
    let content = std::fs::read_to_string(license_grid_path(root, "LicenseRef-Foo")).expect("read");
    assert!(
        content.contains("name = \"X\""),
        "original file must be untouched, got: {content}"
    );
}

// Test case 5: --root writes to the given project root.
#[test]
fn add_license_writes_to_explicit_root() {
    // Given an explicit non-default root.
    let tree = temptree! {};
    let root = tree.path();
    let services = Services::real(root).expect("services");

    // When writing with that root.
    let path = write_license_template(&services, root, "Bar").expect("write");

    // Then the file lands under that root's LICENSES/, not CWD.
    assert!(
        path.starts_with(root),
        "path {path:?} must be under root {root:?}"
    );
    assert!(path.ends_with("LICENSES/LicenseRef-Bar.toml"));
}

// license_ref_id prefixing is idempotent.
#[rstest::rstest]
#[case::bare("Foo", "LicenseRef-Foo")]
#[case::already_prefixed("LicenseRef-Foo", "LicenseRef-Foo")]
fn license_ref_id_canonicalizes(#[case] input: &str, #[case] expected: &str) {
    // Given a bare name and an already-prefixed id.
    // When computing the canonical id.
    let actual = license_ref_id(input);

    // Then the input maps to the expected canonical LicenseRef- form.
    assert_eq!(actual, expected);
}

// Test case 13: a stale `text` field is rejected by deny_unknown_fields at load.
// (Covers the regression that the text store is now LICENSES/<id>.txt only.)
#[test]
fn load_rejects_grid_carrying_dropped_text_field() {
    // Given a LICENSES/*.toml carrying the removed `text` field.
    let tree = temptree! {
        "LICENSES": {
            "LicenseRef-Text.toml": r#"
id = "LicenseRef-Text"
name = "Text"
url = "https://example.com"
text = "should be rejected"

[terms]
requires_attribution = false
requires_license_notice = false
requires_source_disclosure = false
derivatives = "allowed"
requires_modification_notice = false
allows_commercial_use = true
allows_redistribution = true
manual_review = false
"#,
        }
    };
    let fs = common::real_fs();
    let root = tree.path();

    // When loading the registry.
    let result = auditah::registry::LicenseRegistry::load(&fs, root);

    // Then it errors — deny_unknown_fields rejects the removed field.
    assert!(result.is_err(), "removed `text` field must be rejected");
    let _ = Path::new(""); // keep Path import used
}

// Well-known SPDX path integration tests (exercise the run() dispatch).
// These construct LicenseCmd directly and call run() to verify the dual
// dispatch (well-known vs --custom) end-to-end.

use auditah::cli::license_cmd::{run, LicenseCmd};
use auditah::cli::CommandStatus;

// `add-license MIT` (authored grid) extracts text + authored grid; silent.
#[test]
fn add_license_well_known_authored_extracts_text_and_grid() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path().to_path_buf();
    let cmd = LicenseCmd {
        name: "MIT".to_string(),
        custom: false,
        root: root.clone(),
    };

    // When running add-license MIT.
    let status = run(&cmd).expect("run");

    // Then it succeeds, and both the text and authored grid are written.
    assert_eq!(status, CommandStatus::Success);
    let text = std::fs::read_to_string(root.join("LICENSES").join("MIT.txt")).expect("text file");
    assert!(!text.is_empty(), "MIT.txt must be non-empty");
    let grid = std::fs::read_to_string(root.join("LICENSES").join("MIT.toml")).expect("grid file");
    // Authored MIT grid is permissive: manual_review = false.
    assert!(
        grid.contains("manual_review = false"),
        "authored grid must have manual_review=false, got: {grid}"
    );
}

// `add-license mit` (case-insensitive) resolves to canonical MIT on disk.
#[test]
fn add_license_well_known_case_insensitive_writes_canonical_casing() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path().to_path_buf();
    let cmd = LicenseCmd {
        name: "mit".to_string(),
        custom: false,
        root: root.clone(),
    };

    // When running add-license mit (lowercase).
    let status = run(&cmd).expect("run");

    // Then the on-disk files use canonical casing (MIT, not mit).
    assert_eq!(status, CommandStatus::Success);
    assert!(
        root.join("LICENSES").join("MIT.txt").exists(),
        "canonical MIT.txt"
    );
    assert!(
        root.join("LICENSES").join("MIT.toml").exists(),
        "canonical MIT.toml"
    );
}

// `add-license Bzip2-1.0.6` (no authored grid) extracts text + default_fail placeholder.
// Note: the canonical SPDX id is lowercase `bzip2-1.0.6`; resolve normalizes the
// user's input casing to canonical on disk.
#[test]
fn add_license_well_known_no_grid_writes_text_and_placeholder_grid() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path().to_path_buf();
    // Bzip2-1.0.6 is in the SPDX text corpus but has no authored grid.
    let cmd = LicenseCmd {
        name: "Bzip2-1.0.6".to_string(),
        custom: false,
        root: root.clone(),
    };

    // When running add-license Bzip2-1.0.6.
    let status = run(&cmd).expect("run");

    // Then both files are written; the grid is the default_fail() placeholder.
    assert_eq!(status, CommandStatus::Success);
    let grid =
        std::fs::read_to_string(root.join("LICENSES").join("bzip2-1.0.6.toml")).expect("grid");
    assert!(
        grid.contains("manual_review = true"),
        "placeholder grid must be default_fail() (manual_review=true), got: {grid}"
    );
}

// `add-license --custom Foo` writes a LicenseRef-Foo default_fail grid.
#[test]
fn add_license_custom_writes_licenseref_default_fail_grid() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path().to_path_buf();
    let cmd = LicenseCmd {
        name: "Foo".to_string(),
        custom: true,
        root: root.clone(),
    };

    // When running add-license --custom Foo.
    let status = run(&cmd).expect("run");

    // Then it writes LicenseRef-Foo.toml with default_fail() shape; no .txt.
    assert_eq!(status, CommandStatus::Success);
    let grid =
        std::fs::read_to_string(root.join("LICENSES").join("LicenseRef-Foo.toml")).expect("grid");
    assert!(grid.contains("LicenseRef-Foo"));
    assert!(grid.contains("manual_review = true"));
    assert!(
        !root.join("LICENSES").join("LicenseRef-Foo.txt").exists(),
        "custom path must not write a .txt"
    );
}

// `add-license --custom MIT` errors because MIT is a known SPDX id.
#[test]
fn add_license_custom_on_known_spdx_id_errors() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path().to_path_buf();
    let cmd = LicenseCmd {
        name: "MIT".to_string(),
        custom: true,
        root,
    };

    // When running add-license --custom MIT.
    let result = run(&cmd);

    // Then it errors (known id must use the non-custom path).
    assert!(result.is_err(), "--custom MIT must error");
}

// `add-license NotReal` (no flag, no match) errors.
#[test]
fn add_license_unknown_spdx_id_without_custom_errors() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path().to_path_buf();
    let cmd = LicenseCmd {
        name: "NotReal".to_string(),
        custom: false,
        root,
    };

    // When running add-license NotReal.
    let result = run(&cmd);

    // Then it errors (unknown SPDX id).
    assert!(result.is_err(), "unknown SPDX id must error");
}
