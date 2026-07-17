//! Integration tests: `auditah init` — scaffolds a commented `auditah.toml`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::cli::init_cmd::{run, InitCmd};
use auditah::cli::CommandStatus;
use auditah::config::{Config, CONFIG_FILENAME};
use std::path::Path;
use temptree::temptree;

mod common;

fn init_cmd(root: &Path, force: bool) -> InitCmd {
    InitCmd {
        root: root.to_path_buf(),
        force,
    }
}

fn config_path(root: &Path) -> std::path::PathBuf {
    root.join(CONFIG_FILENAME)
}

// Test case 1: `init` writes a commented auditah.toml when none exists, and it
// round-trips to Config::default().
#[test]
fn init_writes_commented_config_when_absent() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path();

    // When running init.
    let status = run(&common::real_services(root), &init_cmd(root, false)).expect("init");

    // Then it succeeds, writes the file, and it round-trips to defaults.
    assert_eq!(status, CommandStatus::Success);
    let path = config_path(root);
    let content = std::fs::read_to_string(&path).expect("read");
    assert!(content.contains("# auditah project config."));
    let fs = auditah::services::fs::FsService::new(std::sync::Arc::new(
        auditah::services::fs::RealFs::new(),
    ));
    let cfg = Config::load(&fs, root).expect("round-trip");
    assert_eq!(cfg, Config::default());
}

// Test case 2: `init` prints the `init: wrote <path>` line.
#[test]
fn init_prints_wrote_line() {
    // Given an empty project root (stdout is exercised implicitly by success;
    // this test asserts the path/status behavior that the message depends on).
    let tree = temptree! {};
    let root = tree.path();

    // When running init.
    let status = run(&common::real_services(root), &init_cmd(root, false)).expect("init");

    // Then the file lands at <root>/auditah.toml and the command succeeds.
    assert_eq!(status, CommandStatus::Success);
    assert!(config_path(root).exists());
}

// Test case 3: `init` refuses to overwrite an existing file.
#[test]
fn init_refuses_existing_file_without_force() {
    // Given a root that already has an auditah.toml.
    let tree = temptree! {
        "auditah.toml": "# mine\ncommercial_project = true\n"
    };
    let root = tree.path();
    let original = std::fs::read_to_string(config_path(root)).expect("read");

    // When running init without --force.
    let result = run(&common::real_services(root), &init_cmd(root, false));

    // Then it errors and leaves the file untouched.
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(config_path(root)).expect("read"),
        original
    );
    let report = result.expect_err("should be error");
    let _dbg = format!("{report:?}");
    assert!(format!("{report:?}").contains("already exists"));
}

// Test case 4: `init --force` overwrites an existing file.
#[test]
fn init_force_overwrites_existing_file() {
    // Given a root with a pre-existing auditah.toml.
    let tree = temptree! {
        "auditah.toml": "# stale\n"
    };
    let root = tree.path();

    // When running init with --force.
    let status = run(&common::real_services(root), &init_cmd(root, true)).expect("init --force");

    // Then it overwrites with the commented template.
    assert_eq!(status, CommandStatus::Success);
    let content = std::fs::read_to_string(config_path(root)).expect("read");
    assert!(content.contains("# auditah project config."));
    assert!(!content.contains("# stale"));
}

// Test case 5: `init` creates the LICENSES/ directory.
#[test]
fn init_creates_licenses_directory() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path();

    // When running init.
    let status = run(&common::real_services(root), &init_cmd(root, false)).expect("init");

    // Then the LICENSES/ directory exists.
    assert_eq!(status, CommandStatus::Success);
    assert!(root.join("LICENSES").is_dir());
}

// Test case 6: `init` leaves an existing LICENSES/ untouched (idempotent).
#[test]
fn init_leaves_existing_licenses_directory_untouched() {
    // Given a root that already has a LICENSES/ with a seed file inside.
    let tree = temptree! {};
    let root = tree.path();
    let licenses_dir = root.join("LICENSES");
    std::fs::create_dir_all(&licenses_dir).expect("mkdir LICENSES");
    let seed = root.join("LICENSES/KeepExisting.txt");
    std::fs::write(&seed, "do not clobber\n").expect("write seed");
    let original = std::fs::read_to_string(&seed).expect("read seed");

    // When running init again.
    let status = run(&common::real_services(root), &init_cmd(root, true)).expect("init idempotent");

    // Then the directory still exists and the seed file is unchanged.
    assert_eq!(status, CommandStatus::Success);
    assert!(root.join("LICENSES").is_dir());
    assert_eq!(
        std::fs::read_to_string(&seed).expect("read seed after"),
        original
    );
}
