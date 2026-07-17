//! Integration tests: `auditah ack` — acknowledges manual-review license ids.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::cli::ack_cmd::{run, AckCmd};
use auditah::cli::CommandStatus;
use auditah::config::{Config, CONFIG_FILENAME};
use std::path::Path;
use temptree::temptree;

mod common;

fn ack_cmd(root: &Path, ids: &[&str]) -> AckCmd {
    AckCmd {
        ids: ids.iter().map(std::string::ToString::to_string).collect(),
        root: root.to_path_buf(),
    }
}

fn config_path(root: &Path) -> std::path::PathBuf {
    root.join(CONFIG_FILENAME)
}

fn load_cfg(root: &Path) -> Config {
    let fs = auditah::services::fs::FsService::new(std::sync::Arc::new(
        auditah::services::fs::RealFs::new(),
    ));
    Config::load(&fs, root).expect("load")
}

// Test case 1: `ack X` on a missing toml creates one with X in
// manual_review_acknowledged.
#[test]
fn ack_creates_toml_with_id_when_absent() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path();

    // When acknowledging a single id.
    let status = run(&ack_cmd(root, &["LicenseRef-StudioEULA"])).expect("ack");

    // Then a new auditah.toml exists containing the id.
    assert_eq!(status, CommandStatus::Success);
    assert!(config_path(root).exists());
    let cfg = load_cfg(root);
    assert_eq!(
        cfg.manual_review_acknowledged,
        vec!["LicenseRef-StudioEULA"]
    );
}

// Test case 2: `ack X` preserves an existing comment when appending.
#[test]
fn ack_preserves_comments_on_existing_toml() {
    // Given an auditah.toml with a user-authored comment and empty ack list.
    let tree = temptree! {
        "auditah.toml": "# my custom header\nmanual_review_acknowledged = []\n"
    };
    let root = tree.path();

    // When acknowledging an id.
    run(&ack_cmd(root, &["LicenseRef-Foo"])).expect("ack");

    // Then the user comment survives and the id is present.
    let content = std::fs::read_to_string(config_path(root)).expect("read");
    assert!(content.contains("# my custom header"));
    assert!(content.contains("LicenseRef-Foo"));
}

// Test case 3: `ack X` is idempotent when X is already present.
#[test]
fn ack_is_idempotent_when_id_already_present() {
    // Given an auditah.toml already listing the id.
    let tree = temptree! {
        "auditah.toml": "manual_review_acknowledged = [\"LicenseRef-Foo\"]\n"
    };
    let root = tree.path();

    // When acknowledging the same id again.
    run(&ack_cmd(root, &["LicenseRef-Foo"])).expect("ack");

    // Then the id appears exactly once.
    let content = std::fs::read_to_string(config_path(root)).expect("read");
    assert_eq!(content.matches("LicenseRef-Foo").count(), 1);
}

// Test case 4: `ack X Y` acknowledges multiple ids at once.
#[test]
fn ack_adds_multiple_ids_in_one_invocation() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path();

    // When acknowledging two ids.
    run(&ack_cmd(root, &["LicenseRef-A", "LicenseRef-B"])).expect("ack");

    // Then both ids are in manual_review_acknowledged.
    let cfg = load_cfg(root);
    assert_eq!(cfg.manual_review_acknowledged.len(), 2);
    assert!(cfg
        .manual_review_acknowledged
        .contains(&"LicenseRef-A".to_string()));
    assert!(cfg
        .manual_review_acknowledged
        .contains(&"LicenseRef-B".to_string()));
}

// Test case 5: `ack <unknown>` still writes the id and succeeds (fail-open).
//
// The warn is emitted to stderr (a side-effect); the observable contract this
// test pins is that an unknown id is written anyway and the command succeeds.
#[test]
fn ack_unknown_id_still_writes_and_succeeds() {
    // Given an empty project root.
    let tree = temptree! {};
    let root = tree.path();

    // When acknowledging an id unknown to the registry and corpus.
    let status = run(&ack_cmd(root, &["Totally-Made-Up-Id-XYZ"])).expect("ack");

    // Then the id is written and the command succeeds.
    assert_eq!(status, CommandStatus::Success);
    let cfg = load_cfg(root);
    assert_eq!(
        cfg.manual_review_acknowledged,
        vec!["Totally-Made-Up-Id-XYZ"]
    );
}

// Test case 6: `ack` preserves other config fields (commercial_project, exclude)
// already set on an existing toml.
#[test]
fn ack_preserves_other_fields_on_existing_toml() {
    // Given an auditah.toml with commercial flag and exclude globs set.
    let tree = temptree! {
        "auditah.toml": "commercial_project = true\nexclude = [\"vendor/**\"]\nmanual_review_acknowledged = []\n"
    };
    let root = tree.path();

    // When acknowledging an id.
    run(&ack_cmd(root, &["LicenseRef-Foo"])).expect("ack");

    // Then the commercial flag and exclude globs are unchanged, and the id is added.
    let cfg = load_cfg(root);
    assert!(cfg.commercial_project);
    assert_eq!(cfg.exclude, vec!["vendor/**"]);
    assert_eq!(cfg.manual_review_acknowledged, vec!["LicenseRef-Foo"]);
}
