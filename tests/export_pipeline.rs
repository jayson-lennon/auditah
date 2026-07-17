//! Integration tests: the `export` pipeline (`export_cmd::run`) end-to-end
//! against a real temp filesystem. One BDD test per behavior, mapped to the
//! plan's acceptance criteria.
//!
//! Every test builds a source *library* project on a temp filesystem: a root
//! with `LICENSES/` (so `run_audit`'s coverage check can resolve licenses) plus
//! asset files and their attribution metadata. The export command runs the full
//! audit gate against the source project, then copies assets + attribution to a
//! target path inside the same tree. The target is never a project of its own —
//! it is a plain sink directory.
//!
//! `temptree!` requires the directory-tree syntax for nested paths
//! (`"pack": { "tree.glb": "..." }`, not `"pack/tree.glb"`) and only accepts
//! string-literal values, so attribution metadata files (whose bodies are built
//! by `record_toml`) are written via `std::fs::write` after the tree is created.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use temptree::temptree;

mod common;
use auditah::cli::export_cmd::{run, ExportCmd};
use auditah::cli::CommandStatus;
use auditah::config::Config;
use auditah::registry::LicenseSpec;
use common::{permissive_terms, seed_license, services_with};

/// A source project with a single resolvable permissive license, configured at
/// `root`. The asset itself is seeded by each test's `temptree!`.
fn source_services(root: &Path, config: Config) -> auditah::services::Services {
    seed_license(root, "LicenseRef-Mit");
    services_with(
        root,
        config,
        [LicenseSpec::new("LicenseRef-Mit").terms(permissive_terms())],
    )
}

/// A complete on-disk attribution record TOML body for `license`.
fn record_toml(title: &str, license: &str) -> String {
    format!(
        "title = \"{title}\"\nauthor = \"Artist\"\nyear = 2020\nlicense = \"{license}\"\nsource = \"https://example.com\"\n"
    )
}

/// Write `record_toml(title, license)` to `path`.
fn write_record(path: &Path, title: &str, license: &str) {
    std::fs::write(path, record_toml(title, license)).expect("write record");
}

/// Build an `ExportCmd` copying `source` -> `target` rooted at `root`.
fn export_cmd(source: &Path, target: &Path, root: &Path, copy_ignored: bool) -> ExportCmd {
    ExportCmd {
        source: source.to_path_buf(),
        target: target.to_path_buf(),
        root: root.to_path_buf(),
        copy_ignored,
    }
}

// Acceptance: file export with adjacent sidecar copies file + sidecar.
#[test]
fn export_file_copies_asset_and_adjacent_sidecar_to_target() {
    // Given a source file with a sidecar and a clean source project.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": { "tree.glb": "tree-bytes" },
    };
    let root = tree.path();
    write_record(
        &root.join("pack/tree.glb.attr.toml"),
        "Tree",
        "LicenseRef-Mit",
    );
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(
        &root.join("pack/tree.glb"),
        &root.join("game/tree.glb"),
        root,
        false,
    );

    // When exporting the file.
    let status = run(&svc, &cmd).expect("export");

    // Then the target file and its sidecar both exist.
    assert_eq!(status, CommandStatus::Success);
    assert_eq!(
        std::fs::read(root.join("game/tree.glb")).unwrap(),
        b"tree-bytes"
    );
    assert!(root.join("game/tree.glb.attr.toml").exists());
}

// Acceptance: file covered only by an ancestor manifest gets a synthesized sidecar.
#[test]
fn export_file_synthesizes_sidecar_from_ancestor_manifest() {
    // Given a file covered only by a directory manifest one level up.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": { "leaves": { "tree.glb": "tree-bytes" } },
    };
    let root = tree.path();
    write_record(
        &root.join("pack/_manifest.toml"),
        "Nature Pack",
        "LicenseRef-Mit",
    );
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(
        &root.join("pack/leaves/tree.glb"),
        &root.join("game/tree.glb"),
        root,
        false,
    );

    // When exporting the file.
    run(&svc, &cmd).expect("export");

    // Then a sidecar is synthesized at the target with the provenance title.
    let sidecar =
        std::fs::read_to_string(root.join("game/tree.glb.attr.toml")).expect("read sidecar");
    assert!(
        sidecar.contains("tree (from pack)"),
        "expected provenance title 'tree (from pack)', got: {sidecar}"
    );
}

// Acceptance: dir export with a local manifest copies it to the target root.
#[test]
fn export_dir_with_local_manifest_copies_it_to_target_root() {
    // Given a source dir with its own manifest and an asset.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": { "tree.glb": "tree-bytes" },
    };
    let root = tree.path();
    write_record(&root.join("pack/_manifest.toml"), "Pack", "LicenseRef-Mit");
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(&root.join("pack"), &root.join("game/pack"), root, false);

    // When exporting the directory.
    run(&svc, &cmd).expect("export");

    // Then the target root has the copied manifest and the asset.
    assert!(root.join("game/pack/_manifest.toml").exists());
    assert!(root.join("game/pack/tree.glb").exists());
}

// Acceptance: dir export covered by an ancestor manifest synthesizes one at target root.
#[test]
fn export_dir_synthesizes_manifest_at_target_root_from_ancestor() {
    // Given a dir covered by a manifest two levels up.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "lib": { "sub": { "tree.glb": "tree-bytes" } },
    };
    let root = tree.path();
    write_record(
        &root.join("lib/_manifest.toml"),
        "Library",
        "LicenseRef-Mit",
    );
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(&root.join("lib/sub"), &root.join("game/sub"), root, false);

    // When exporting the directory.
    run(&svc, &cmd).expect("export");

    // Then a synthesized manifest exists at the target root.
    assert!(root.join("game/sub/_manifest.toml").exists());
}

// Acceptance: nested subdir manifests are preserved in the target tree.
#[test]
fn export_dir_preserves_nested_subdir_manifests() {
    // Given a dir with a local root manifest and a subdir with its own manifest.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": {
            "tree.glb": "tree-bytes",
            "inner": { "rock.glb": "rock-bytes" },
        },
    };
    let root = tree.path();
    write_record(&root.join("pack/_manifest.toml"), "Pack", "LicenseRef-Mit");
    write_record(
        &root.join("pack/inner/_manifest.toml"),
        "Inner",
        "LicenseRef-Mit",
    );
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(&root.join("pack"), &root.join("game/pack"), root, false);

    // When exporting the directory.
    run(&svc, &cmd).expect("export");

    // Then the nested subdir manifest is preserved in the target.
    assert!(root.join("game/pack/inner/_manifest.toml").exists());
}

// Acceptance: any source-project audit failure aborts the export and writes nothing.
#[test]
fn export_aborts_when_source_project_has_audit_failure() {
    // Given a source project with an uncovered (unlicensed) asset elsewhere.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": { "tree.glb": "tree-bytes" },
        "other": { "orphan.glb": "orphan-bytes" },
    };
    let root = tree.path();
    write_record(
        &root.join("pack/tree.glb.attr.toml"),
        "Tree",
        "LicenseRef-Mit",
    );
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(
        &root.join("pack/tree.glb"),
        &root.join("game/tree.glb"),
        root,
        false,
    );

    // When exporting the clean file.
    let result = run(&svc, &cmd);

    // Then the command errors and nothing was written to the target.
    assert!(result.is_err(), "expected export to abort on audit failure");
    assert!(!root.join("game/tree.glb").exists());
}

// Acceptance: single source file matching an exclude glob warns and copies nothing.
#[test]
fn export_warns_and_skips_when_single_file_matches_exclude_glob() {
    // Given a single source file that the merged matcher excludes (.git path).
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        ".git": { "config": "git-config" },
    };
    let root = tree.path();
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(
        &root.join(".git/config"),
        &root.join("game/config"),
        root,
        false,
    );

    // When exporting the excluded single file.
    let status = run(&svc, &cmd).expect("export returns Ok despite the skip");

    // Then the command succeeds (warn-and-skip) and the target was not created.
    assert_eq!(status, CommandStatus::Success);
    assert!(!root.join("game/config").exists());
}

// Acceptance: --copy-ignored copies matcher-excluded files into the target.
#[test]
fn export_with_copy_ignored_copies_excluded_files() {
    // Given a dir containing a build-output file the matcher excludes.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": {
            "tree.glb": "tree-bytes",
            "target": { "build.o": "build-bytes" },
        },
    };
    let root = tree.path();
    write_record(&root.join("pack/_manifest.toml"), "Pack", "LicenseRef-Mit");
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(&root.join("pack"), &root.join("game/pack"), root, true);

    // When exporting with --copy-ignored.
    run(&svc, &cmd).expect("export");

    // Then the otherwise-excluded build file is copied to the target.
    assert!(root.join("game/pack/target/build.o").exists());
}

// Acceptance: --copy-ignored also copies an EXCLUDED SINGLE-FILE source (edge1, decision b).
#[test]
fn export_copy_ignored_copies_excluded_single_file() {
    // Given a single source file the matcher excludes (.git path) with a sidecar
    // so the audit gate's resolution passes and export_file can proceed.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        ".git": { "config": "git-config" },
    };
    let root = tree.path();
    write_record(
        &root.join(".git/config.attr.toml"),
        "Config",
        "LicenseRef-Mit",
    );
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(
        &root.join(".git/config"),
        &root.join("game/config"),
        root,
        true,
    );

    // When exporting the excluded single file with --copy-ignored.
    run(&svc, &cmd).expect("export");

    // Then the otherwise-skipped file is copied to the target.
    assert!(root.join("game/config").exists());
}

// Acceptance: sidecars always travel with their assets even though the matcher excludes them.
#[test]
fn export_dir_always_copies_sidecars() {
    // Given a dir with an asset and its sidecar.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": { "tree.glb": "tree-bytes" },
    };
    let root = tree.path();
    write_record(&root.join("pack/_manifest.toml"), "Pack", "LicenseRef-Mit");
    write_record(
        &root.join("pack/tree.glb.attr.toml"),
        "Tree",
        "LicenseRef-Mit",
    );
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(&root.join("pack"), &root.join("game/pack"), root, false);

    // When exporting the directory.
    run(&svc, &cmd).expect("export");

    // Then the sidecar is present in the target despite the matcher excluding it.
    assert!(root.join("game/pack/tree.glb.attr.toml").exists());
}

// Acceptance: the target project is never read, loaded, or modified — LICENSES never copied.
#[test]
fn export_never_copies_licenses_dir_to_target() {
    // Given a source dir containing a nested LICENSES directory.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": {
            "tree.glb": "tree-bytes",
            "LICENSES": { "MIT.txt": "license text" },
        },
    };
    let root = tree.path();
    write_record(&root.join("pack/_manifest.toml"), "Pack", "LicenseRef-Mit");
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(&root.join("pack"), &root.join("game/pack"), root, false);

    // When exporting the directory.
    run(&svc, &cmd).expect("export");

    // Then the LICENSES directory was not copied into the target.
    assert!(!root.join("game/pack/LICENSES/MIT.txt").exists());
}

// Acceptance: a binary asset is copied byte-identical to the target.
#[test]
fn export_file_copies_binary_asset_byte_identical() {
    // Given a binary asset with non-UTF-8 bytes and a sidecar.
    let tree = temptree! {
        "LICENSES": { ".gitkeep": "" },
        "pack": { "model.glb": "\u{0}\u{1}\u{ff}\u{fe}binary\u{0}" },
    };
    let root = tree.path();
    write_record(
        &root.join("pack/model.glb.attr.toml"),
        "Model",
        "LicenseRef-Mit",
    );
    let svc = source_services(root, Config::default());
    let cmd = export_cmd(
        &root.join("pack/model.glb"),
        &root.join("game/model.glb"),
        root,
        false,
    );

    // When exporting the binary file.
    run(&svc, &cmd).expect("export");

    // Then the target bytes match the source exactly.
    let src = std::fs::read(root.join("pack/model.glb")).unwrap();
    let dst = std::fs::read(root.join("game/model.glb")).unwrap();
    assert_eq!(src, dst);
}
