//! Integration tests for the merged `auditah license` command: file target →
//! sidecar, directory target → manifest, both with provisioning into `LICENSES/`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use auditah::cli::license_cmd::{run, LicenseCmd};
use auditah::cli::CommandStatus;
use temptree::temptree;

/// A `LicenseCmd` builder with the required `--id`/`--author` set; the target
/// and root are the per-test variables.
fn cmd(target: &Path, root: &Path, id: &str, author: &str) -> LicenseCmd {
    LicenseCmd {
        target: target.to_path_buf(),
        id: id.to_string(),
        author: author.to_string(),
        title: None,
        year: Some(2020),
        source: None,
        modified: false,
        root: Some(root.to_path_buf()),
    }
}

// A file target writes a `<file>.attr.toml` sidecar.
#[test]
fn license_file_target_writes_attr_sidecar() {
    // Given a project with an asset file and a LICENSES/ dir.
    let tree = temptree! {
        "sword.glb": "binary",
        "LICENSES": {},
    };
    let root = tree.path();
    std::fs::write(
        root.join("LICENSES/LicenseRef-Asset.toml"),
        "id = \"LicenseRef-Asset\"\nname = \"x\"\nurl = \"\"\n[terms]\nrequires_attribution = false\nrequires_license_notice = false\nrequires_source_disclosure = false\nderivatives = \"allowed\"\nrequires_modification_notice = false\nallows_commercial_use = true\nallows_redistribution = true\nmanual_review = false\n",
    )
    .unwrap();

    // When running `license sword.glb --id LicenseRef-Asset --author A`.
    let status = run(
        &cmd(&root.join("sword.glb"), root, "LicenseRef-Asset", "A"),
        root,
    )
    .expect("run");

    // Then it succeeds and writes the sidecar next to the file.
    assert_eq!(status, CommandStatus::Success);
    assert!(
        root.join("sword.glb.attr.toml").exists(),
        "sidecar must be written"
    );
}

// A directory target writes a `_manifest.toml` in that directory.
#[test]
fn license_dir_target_writes_manifest() {
    // Given a project with a pack directory and a LICENSES/ dir.
    let tree = temptree! {
        "pack": { "a.glb": "binary" },
        "LICENSES": {},
    };
    let root = tree.path();
    std::fs::write(
        root.join("LICENSES/LicenseRef-Asset.toml"),
        "id = \"LicenseRef-Asset\"\nname = \"x\"\nurl = \"\"\n[terms]\nrequires_attribution = false\nrequires_license_notice = false\nrequires_source_disclosure = false\nderivatives = \"allowed\"\nrequires_modification_notice = false\nallows_commercial_use = true\nallows_redistribution = true\nmanual_review = false\n",
    )
    .unwrap();

    // When running `license pack --id LicenseRef-Asset --author A`.
    let status = run(
        &cmd(&root.join("pack"), root, "LicenseRef-Asset", "A"),
        root,
    )
    .expect("run");

    // Then it succeeds and writes the manifest inside the target directory.
    assert_eq!(status, CommandStatus::Success);
    assert!(
        root.join("pack/_manifest.toml").exists(),
        "manifest must be written inside the dir"
    );
}

// `--modified` on a directory target is a hard error (semantically meaningless).
#[test]
fn license_dir_target_rejects_modified_flag() {
    // Given a directory target.
    let tree = temptree! {
        "pack": {},
        "LICENSES": {},
    };
    let root = tree.path();
    let mut cmd = cmd(&root.join("pack"), root, "LicenseRef-Asset", "A");
    cmd.modified = true;

    // When running `license pack --modified`.
    let result = run(&cmd, root);

    // Then it errors (a directory manifest cannot be "modified").
    assert!(
        result.is_err(),
        "--modified on a directory must be a hard error"
    );
}

// Both branches provision a well-known id into LICENSES/ when absent.
#[test]
fn license_provisions_well_known_id_into_licenses_when_absent() {
    // Given a project whose LICENSES/ is empty (MIT not yet present).
    let tree = temptree! {
        "sword.glb": "binary",
        "LICENSES": {},
    };
    let root = tree.path();

    // When running `license sword.glb --id MIT --author A`.
    let status = run(&cmd(&root.join("sword.glb"), root, "MIT", "A"), root).expect("run");

    // Then MIT is provisioned (text + grid) into LICENSES/.
    assert_eq!(status, CommandStatus::Success);
    assert!(
        root.join("LICENSES/MIT.txt").exists(),
        "MIT.txt must be provisioned"
    );
    assert!(
        root.join("LICENSES/MIT.toml").exists(),
        "MIT.toml grid must be provisioned"
    );
}

// An unknown id that is absent from LICENSES/ errors with an add-license hint.
#[test]
fn license_errors_on_unknown_id_absent_from_licenses() {
    // Given a project whose LICENSES/ has no StudioEULA grid.
    let tree = temptree! {
        "sword.glb": "binary",
        "LICENSES": {},
    };
    let root = tree.path();

    // When running `license sword.glb --id StudioEULA --author A`.
    let result = run(&cmd(&root.join("sword.glb"), root, "StudioEULA", "A"), root);

    // Then it errors and points the user at `add-license --custom`.
    let report = result.expect_err("unknown id must error");
    let rendered = format!("{report:?}");
    assert!(
        rendered.contains("--custom"),
        "error must mention add-license --custom: {rendered}"
    );
    assert!(
        rendered.contains("StudioEULA"),
        "error must name the offending id: {rendered}"
    );
}

// `--root` overrides LICENSES/ discovery: the project root is used directly
// even when no ancestor of the target carries a LICENSES/.
#[test]
fn license_root_override_locates_licenses_dir() {
    // Given a project root with LICENSES/ and a target outside any walk-up path.
    let tree = temptree! {
        "project": {
            "LICENSES": {},
        },
        "isolated.glb": "binary",
    };
    let root = tree.path();

    // When running `license isolated.glb --root project --id MIT --author A`.
    let status = run(
        &cmd(
            &root.join("isolated.glb"),
            &root.join("project"),
            "MIT",
            "A",
        ),
        root,
    )
    .expect("run");

    // Then it succeeds and provisions MIT under the overridden project root.
    assert_eq!(status, CommandStatus::Success);
    assert!(
        root.join("project/LICENSES/MIT.txt").exists(),
        "MIT must be provisioned under --root/LICENSES"
    );
}
