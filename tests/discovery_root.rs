//! Integration tests: project-root discovery (`find_project_root`).
//!
//! Verifies the LICENSES-dependent commands (`audit`, `generate`, `license provision`,
//! and the merged `license` command) resolve an *ancestor* `LICENSES/` when
//! invoked from a subdirectory, and hard-error (no fallback) when none exists.
//! The hard-error message contract is covered in `error_scenarios.rs`; this
//! file covers the resolve-upward success path end-to-end through each command's `run`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use auditah::cli::audit_cmd::{run as audit_run, AuditCmd};
use auditah::cli::generate_cmd::{run as generate_run, GenerateCmd};
use auditah::cli::license_assign_cmd::{run as license_run, LicenseAssignCmd};
use auditah::cli::license_provision_cmd::{run as license_provision_run, LicenseProvisionCmd};
use auditah::cli::CommandStatus;
use temptree::temptree;

mod common;

// Build a Services by resolving an ancestor LICENSES/ from `start` anchored at
// `cwd` — exactly the wiring `main`'s `dispatch` performs before calling run().
fn resolve_services(cwd: &Path, start: &Path) -> auditah::services::Services {
    common::resolve_services(cwd, start)
}

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

    // When auditing with --root pointing at the subdir (LICENSES is above it):
    // dispatch resolves the ancestor root and builds Services from it.
    let cmd = AuditCmd::default();
    let services = resolve_services(root, &root.join("sub"));

    // Then it resolves the ancestor project root and audits cleanly (Success).
    let status = audit_run(&services, &cmd).expect("audit should resolve ancestor LICENSES");
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

    // When auditing with a relative ".", anchored at the injected cwd (the subdir)
    // — no process-cwd mutation.
    let cmd = AuditCmd {
        root: PathBuf::from("."),
        ..Default::default()
    };
    let services = resolve_services(&root.join("sub"), &cmd.root);

    // Then it still succeeds — the relative path walks the real ancestor chain.
    let status = audit_run(&services, &cmd)
        .expect("audit should resolve ancestor LICENSES via relative root");
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
        root: root.join("sub"),
        output_credits: Some(out_credits.clone()),
        output_notices: Some(out_notices.clone()),
        output_bom: Some(out_bom.clone()),
    };
    let services = resolve_services(root, &cmd.root);

    // Then it resolves the ancestor root and writes all three artifacts.
    let status = generate_run(&services, &cmd).expect("generate should resolve ancestor LICENSES");
    assert_eq!(
        status,
        CommandStatus::Success,
        "subdir generate must resolve the ancestor LICENSES, not fail"
    );
    assert!(out_credits.exists(), "CREDITS not written");
    assert!(out_notices.exists(), "NOTICES not written");
    assert!(out_bom.exists(), "BOM not written");
}

// license provision resolves a LICENSES/ located above --root and writes the
// license grid+text into the ANCESTOR project's LICENSES/, not the subdir.
#[test]
fn license_provision_resolves_ancestor_licenses_from_subdir() {
    // Given a project with LICENSES/ at the root and a subdir beneath it.
    let tree = temptree! {
        "LICENSES": {},
        "sub": {},
    };
    let root = tree.path();

    // When adding a well-known license (MIT) with --root pointing at the subdir.
    let cmd = LicenseProvisionCmd {
        name: "MIT".to_string(),
        custom: false,
        root: root.join("sub"),
    };
    let services = resolve_services(root, &cmd.root);
    let status = license_provision_run(&services, &cmd)
        .expect("license provision should resolve ancestor LICENSES");

    // Then it resolves the ancestor root and writes MIT into the ancestor LICENSES/.
    assert_eq!(
        status,
        CommandStatus::Success,
        "subdir license provision must resolve the ancestor LICENSES, not fail"
    );
    assert!(
        root.join("LICENSES/MIT.toml").exists(),
        "MIT grid must be written to the ANCESTOR LICENSES/, not the subdir"
    );
    assert!(
        !root.join("sub/LICENSES").exists(),
        "license provision must not create a LICENSES/ in the subdir"
    );
}

// `license <dir>` hard-errors when the target has no ancestor LICENSES/.
#[test]
fn license_dir_target_no_licenses_dir_returns_err() {
    // Given a directory with no LICENSES/ anywhere up the tree.
    let tree = temptree! {
        "pack": {},
    };
    let root = tree.path();

    // When dispatching `license` against the pack dir: resolve_or_error finds no
    // LICENSES/ ancestor and hard-errors (no Services is built).
    let result = auditah::project::resolve_or_error(root, &root.join("pack"));

    // Then it returns Err pointing the user at `auditah init`.
    let err = result.expect_err("must hard-error when no ancestor LICENSES/");
    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("auditah init"),
        "error must mention `auditah init`, got: {rendered}"
    );
}

// `license <dir>` discovers an ancestor LICENSES/ from the target, provisions
// the license there, and writes the manifest into the target directory. This
// replaces the old cwd-coupled init-pack success path with the target-based
// directory branch of the merged `license` command.
#[test]
fn license_dir_resolves_ancestor_licenses_and_writes_manifest_in_target() {
    // Given a project with LICENSES/ at the root and a subdir beneath it.
    let tree = temptree! {
        "LICENSES": {},
        "sub": {},
    };
    let root = tree.path();
    let target = root.join("sub");
    let cmd = LicenseAssignCmd {
        target: target.clone(),
        id: "MIT".to_string(),
        author: "Artist".to_string(),
        title: None,
        year: None,
        source: None,
        modified: false,
        root: None,
    };

    // When running `license sub` (discovery walks up from the target subdir
    // to find LICENSES/, no process-cwd mutation).
    let services = resolve_services(root, &target);
    let status = license_run(&services, &cmd)
        .expect("license <dir> should resolve ancestor LICENSES and succeed");

    // Then it returns Success.
    assert_eq!(
        status,
        CommandStatus::Success,
        "subdir license <dir> must resolve the ancestor LICENSES, not fail"
    );
    // And the manifest is written into the target directory (the subdir).
    assert!(
        target.join("_manifest.toml").exists(),
        "manifest must be written into the target dir (subdir)"
    );
    // And the license was provisioned into the ANCESTOR LICENSES/.
    assert!(
        root.join("LICENSES/MIT.toml").exists(),
        "MIT grid must be provisioned into the ANCESTOR LICENSES/"
    );
}
