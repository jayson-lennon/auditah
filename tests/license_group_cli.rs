//! Integration tests for the restructured license command surface.
//!
//! Covers the three behaviors the refactor introduced that were not already
//! exercised by the renamed pipeline tests:
//!   1. bare `auditah license` (no subcommand) exits non-zero and prints help;
//!   2. the top-level `assign` shortcut produces a byte-identical sidecar to
//!      `license assign` for a file target;
//!   3. the top-level `assign` shortcut produces a byte-identical manifest to
//!      `license assign` for a directory target.
//!
//! Every other refactor behavior (provision well-known/custom/case-insensitive,
//! overwrite refusal, ancestor discovery, no-LICENSES hard error, ack appends,
//! unknown-id hint) is already covered by `provision_pipeline`,
//! `license_merge_pipeline`, `discovery_root`, `error_scenarios`, and
//! `ack_pipeline` under the new symbols.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Stdio};
use std::str;

use tempfile::TempDir;

/// Path to the built `auditah` binary, as injected by cargo.
fn bin() -> String {
    env!("CARGO_BIN_EXE_auditah").to_string()
}

/// Run `auditah <args...>` with `--root` pointed at `root`, capturing
/// stdout+stderr. Returns (`exit_code`, `stdout`, `stderr`) as UTF-8 strings.
fn run_in(root: &TempDir, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(bin())
        .args(args)
        .arg("--root")
        .arg(root.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn auditah");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

// Bare `auditah license` (no subcommand) errors and prints help.
#[test]
fn bare_license_without_subcommand_errors_and_prints_help() {
    // Given an initialized project (so we isolate the parse failure to the
    // missing subcommand, not a missing LICENSES/).
    let root = TempDir::new().expect("tempdir");
    let (_, _, _) = run_in(&root, &["init"]);

    // When invoking `license` with no subcommand.
    let (code, _stdout, stderr) = run_in(&root, &["license"]);

    // Then it exits non-zero and stderr shows the subcommand usage.
    assert_ne!(code, 0, "bare `license` must exit non-zero");
    assert!(
        stderr.contains("Usage: auditah license <COMMAND>"),
        "stderr should show the license subcommand usage, got: {stderr}"
    );
}

// The top-level `assign` shortcut writes a byte-identical sidecar to
// `license assign` for a file target.
#[test]
fn top_level_assign_writes_same_sidecar_as_license_assign_for_file() {
    // Given two identical initialized projects, each with an asset file.
    let root_a = TempDir::new().expect("tempdir");
    let root_b = TempDir::new().expect("tempdir");
    let asset_a = root_a.path().join("sword.glb");
    let asset_b = root_b.path().join("sword.glb");
    std::fs::write(&asset_a, "binary").expect("write asset");
    std::fs::write(&asset_b, "binary").expect("write asset");
    let _ = run_in(&root_a, &["init"]);
    let _ = run_in(&root_b, &["init"]);

    // When assigning via the top-level shortcut in one, and via the group in the other.
    let _ = run_in(
        &root_a,
        &[
            "assign",
            asset_a.to_str().unwrap(),
            "--id",
            "MIT",
            "--author",
            "Quaternius",
        ],
    );
    let _ = run_in(
        &root_b,
        &[
            "license",
            "assign",
            asset_b.to_str().unwrap(),
            "--id",
            "MIT",
            "--author",
            "Quaternius",
        ],
    );

    // Then both sidecars are byte-identical.
    let a = std::fs::read_to_string(root_a.path().join("sword.glb.attr.toml"))
        .expect("shortcut sidecar");
    let b =
        std::fs::read_to_string(root_b.path().join("sword.glb.attr.toml")).expect("group sidecar");
    assert_eq!(
        a, b,
        "top-level assign must match `license assign` byte-for-byte"
    );
}

// The top-level `assign` shortcut writes a byte-identical manifest to
// `license assign` for a directory target.
#[test]
fn top_level_assign_writes_same_manifest_as_license_assign_for_dir() {
    // Given two identical initialized projects, each with an asset directory.
    let root_a = TempDir::new().expect("tempdir");
    let root_b = TempDir::new().expect("tempdir");
    let pack_a = root_a.path().join("pack");
    let pack_b = root_b.path().join("pack");
    std::fs::create_dir(&pack_a).expect("mkdir");
    std::fs::create_dir(&pack_b).expect("mkdir");
    let _ = run_in(&root_a, &["init"]);
    let _ = run_in(&root_b, &["init"]);

    // When assigning the directory via the shortcut in one, and via the group in the other.
    let _ = run_in(
        &root_a,
        &[
            "assign",
            pack_a.to_str().unwrap(),
            "--id",
            "CC0-1.0",
            "--author",
            "Quaternius",
        ],
    );
    let _ = run_in(
        &root_b,
        &[
            "license",
            "assign",
            pack_b.to_str().unwrap(),
            "--id",
            "CC0-1.0",
            "--author",
            "Quaternius",
        ],
    );

    // Then both manifests are byte-identical.
    let a = std::fs::read_to_string(root_a.path().join("pack/_manifest.toml"))
        .expect("shortcut manifest");
    let b =
        std::fs::read_to_string(root_b.path().join("pack/_manifest.toml")).expect("group manifest");
    assert_eq!(
        a, b,
        "top-level assign must match `license assign` byte-for-byte"
    );
}
