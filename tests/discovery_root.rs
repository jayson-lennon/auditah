//! Integration tests: project-root discovery (`find_project_root`).
//!
//! Verifies the LICENSES-dependent commands (`audit`, `generate`, `license`,
//! `init-pack`) resolve an *ancestor* `LICENSES/` when invoked from a
//! subdirectory, and hard-error (no fallback) when none exists. The hard-error
//! message contract is covered in `error_scenarios.rs`; this file covers the
//! resolve-upward success path end-to-end through each command's `run`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use auditah::cli::audit_cmd::{run as audit_run, AuditCmd};
use auditah::cli::generate_cmd::{run as generate_run, GenerateCmd};
use auditah::cli::init_pack_cmd::{run as init_pack_run, InitPackCmd};
use auditah::cli::license_cmd::{run as license_run, LicenseCmd};
use auditah::cli::CommandStatus;
use temptree::temptree;

mod common;

// audit resolves a LICENSES/ located above --root and audits against it.
#[test]
fn audit_resolves_ancestor_licenses_from_subdir() {
    // Given a project with LICENSES/ + a licensed asset at the root, and a
    // subdir that contains its own asset whose sidecar references the same
    // license.
    let tree = temptree! {
        "LICENSES": {},
        "root.glb": "binary",
        "root.glb.attr.toml": r#"
title = "Root"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"
"#,
        "sub": {
            "deep.glb": "binary",
            "deep.glb.attr.toml": r#"
title = "Deep"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"
"#,
        },
    };
    let root = tree.path();
    // init creates LICENSES/; seed the license the sidecars reference.
    common::seed_license(root, "LicenseRef-Asset");

    // When auditing with --root pointing at the subdir (LICENSES is above it).
    let cmd = AuditCmd {
        root: root.join("sub").clone(),
        ..Default::default()
    };

    // Then it resolves the ancestor project root and audits cleanly (Success).
    let status = audit_run(&cmd, root).expect("audit should resolve ancestor LICENSES");
    assert_eq!(
        status,
        CommandStatus::Success,
        "subdir invocation must resolve the ancestor LICENSES, not fail"
    );
}

// audit resolves the ancestor LICENSES even when --root is a RELATIVE path.
// Regression: `Path::new(".").parent()` returns `Some("")`, which used to
// terminate the walk early; resolve_or_error now canonicalizes the start.
#[test]
fn audit_resolves_ancestor_licenses_with_relative_root() {
    // Given a project rooted at the CWD with LICENSES/ and a licensed asset, and
    // a subdir asset whose sidecar references the same license.
    let tree = temptree! {
        "LICENSES": {},
        "root.glb": "binary",
        "root.glb.attr.toml": r#"
title = "Root"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"
"#,
        "sub": {
            "deep.glb": "binary",
            "deep.glb.attr.toml": r#"
title = "Deep"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"
"#,
        },
    };
    let root = tree.path();
    common::seed_license(root, "LicenseRef-Asset");

    // When auditing with --root as a relative ".", anchored at the injected
    // cwd (the subdir) — no process-cwd mutation.
    let cmd = AuditCmd {
        root: PathBuf::from("."),
        ..Default::default()
    };
    let status = audit_run(&cmd, &root.join("sub"))
        .expect("audit should resolve ancestor LICENSES via relative root");

    // Then it still succeeds — the relative path walks the real ancestor chain.
    assert_eq!(
        status,
        CommandStatus::Success,
        "relative --root must canonicalize before walking ancestors"
    );
}
// generate resolves a LICENSES/ located above --root and writes its artifacts
// against the ancestor project root.
#[test]
fn generate_resolves_ancestor_licenses_from_subdir() {
    // Given a project with LICENSES/ + a notice-required asset at the root,
    // and a subdir that contains its own asset referencing the same license.
    let tree = temptree! {
        "LICENSES": {},
        "root.glb": "binary",
        "root.glb.attr.toml": r#"
title = "Root"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"
"#,
        "sub": {
            "deep.glb": "binary",
            "deep.glb.attr.toml": r#"
title = "Deep"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"
"#,
        },
    };
    let root = tree.path();
    common::seed_license(root, "LicenseRef-Asset");

    // When generating with --root pointing at the subdir.
    let out_credits = root.join("c.md");
    let out_notices = root.join("n.md");
    let out_bom = root.join("b.md");
    let cmd = GenerateCmd {
        root: root.join("sub").clone(),
        output_credits: Some(out_credits.clone()),
        output_notices: Some(out_notices.clone()),
        output_bom: Some(out_bom.clone()),
    };

    // Then it resolves the ancestor root and writes all three artifacts.
    let status = generate_run(&cmd, root).expect("generate should resolve ancestor LICENSES");
    assert_eq!(
        status,
        CommandStatus::Success,
        "subdir generate must resolve the ancestor LICENSES, not fail"
    );
    assert!(out_credits.exists(), "CREDITS not written");
    assert!(out_notices.exists(), "NOTICES not written");
    assert!(out_bom.exists(), "BOM not written");
}

// add-license resolves a LICENSES/ located above --root and writes the
// license grid+text into the ANCESTOR project's LICENSES/, not the subdir.
#[test]
fn license_cmd_resolves_ancestor_licenses_from_subdir() {
    // Given a project with LICENSES/ at the root and a subdir beneath it.
    let tree = temptree! {
        "LICENSES": {},
        "sub": {},
    };
    let root = tree.path();

    // When adding a well-known license (MIT) with --root pointing at the subdir.
    let cmd = LicenseCmd {
        name: "MIT".to_string(),
        custom: false,
        root: root.join("sub").clone(),
    };
    let status = license_run(&cmd, root).expect("add-license should resolve ancestor LICENSES");

    // Then it resolves the ancestor root and writes MIT into the ancestor LICENSES/.
    assert_eq!(
        status,
        CommandStatus::Success,
        "subdir add-license must resolve the ancestor LICENSES, not fail"
    );
    assert!(
        root.join("LICENSES/MIT.toml").exists(),
        "MIT grid must be written to the ANCESTOR LICENSES/, not the subdir"
    );
    assert!(
        !root.join("sub/LICENSES").exists(),
        "add-license must not create a LICENSES/ in the subdir"
    );
}

// init-pack hard-errors when the cwd has no ancestor LICENSES/ (cwd-coupled:
// it walks up from current_dir(), not a --root flag).
#[test]
fn init_pack_no_licenses_dir_returns_err() {
    // Given a directory with no LICENSES/ anywhere up the tree.
    let tree = temptree! {
        "sword.glb": "binary",
    };
    let root = tree.path();
    let cmd = InitPackCmd {
        license: "MIT".to_string(),
        author: "Artist".to_string(),
        year: None,
        title: None,
        source: None,
    };

    // When running init-pack with the injected cwd set to that directory
    // (no process-cwd mutation).
    let result = init_pack_run(&cmd, root);

    // Then it returns Err pointing the user at `auditah init`.
    assert!(
        result.is_err(),
        "init-pack must hard-error when no ancestor LICENSES/ exists"
    );
    let rendered = format!("{:?}", result.expect_err("err"));
    assert!(
        rendered.contains("auditah init"),
        "error must mention `auditah init`, got: {rendered}"
    );
}

// init-pack discovers an ancestor LICENSES/ from the injected cwd (not
// process cwd), provisions the license there, and writes the manifest into
// the injected cwd. Locks the full decoupled success path.
#[test]
fn init_pack_resolves_ancestor_licenses_and_writes_manifest_in_cwd() {
    // Given a project with LICENSES/ at the root and a subdir beneath it.
    let tree = temptree! {
        "LICENSES": {},
        "sub": {},
    };
    let root = tree.path();
    let cmd = InitPackCmd {
        license: "MIT".to_string(),
        author: "Artist".to_string(),
        year: None,
        title: None,
        source: None,
    };

    // When running init-pack with the injected cwd pointing at the subdir
    // (discovery must walk up from cwd to find LICENSES/, no process-cwd mutation).
    let status = init_pack_run(&cmd, &root.join("sub"))
        .expect("init-pack should resolve ancestor LICENSES and succeed");

    // Then it returns Success.
    assert_eq!(
        status,
        CommandStatus::Success,
        "subdir init-pack must resolve the ancestor LICENSES, not fail"
    );
    // And the manifest is written into the injected cwd (the subdir), not the
    // project root.
    assert!(
        root.join("sub/_manifest.toml").exists(),
        "manifest must be written to the injected cwd (subdir)"
    );
    // And the license was provisioned into the ANCESTOR LICENSES/.
    assert!(
        root.join("LICENSES/MIT.toml").exists(),
        "MIT grid must be provisioned into the ANCESTOR LICENSES/"
    );
}
