//! Integration tests: observable CLI output via the real binary.
//!
//! These tests spawn the `auditah` binary (`CARGO_BIN_EXE_auditah`) with piped
//! stdout/stderr and assert on what the user actually sees — the printed
//! `<verb>: wrote <path>` lines, the stderr warning for unknown ids, and the
//! exit code for technical failures. They complement the in-process
//! `init_pipeline` / `ack_pipeline` tests, which assert on file contents but
//! cannot observe stdout/stderr.
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

// Test case 1: `init` prints the `init: wrote <path>` line to stdout.
#[test]
fn init_prints_wrote_line_to_stdout() {
    // Given an empty temp project root.
    let root = TempDir::new().expect("tempdir");

    // When running `auditah init`.
    let (code, stdout, _stderr) = run_in(&root, &["init"]);

    // Then stdout announces the write with the canonical verb and the path.
    assert_eq!(code, 0);
    assert!(
        stdout.contains("init: wrote"),
        "expected `init: wrote` in stdout, got: {stdout:?}"
    );
    assert!(
        stdout.contains("auditah.toml"),
        "expected the config filename in stdout, got: {stdout:?}"
    );
}

// Test case 2: `ack` prints its `license ack: wrote`/`license ack: updated` line to stdout.
#[test]
fn ack_prints_wrote_line_to_stdout() {
    // Given an empty temp project root.
    let root = TempDir::new().expect("tempdir");

    // When running `auditah ack` on a missing config (create path).
    let (code, stdout, _stderr) = run_in(&root, &["license", "ack", "LicenseRef-Foo"]);

    // Then stdout announces the write with the canonical ack verb.
    assert_eq!(code, 0);
    assert!(
        stdout.contains("license ack:"),
        "expected `license ack:` in stdout, got: {stdout:?}"
    );
    assert!(
        stdout.contains("auditah.toml"),
        "expected the config filename in stdout, got: {stdout:?}"
    );
}

// Test case 3: `ack` warns to stderr for an id unknown to both the registry
// and the well-known corpus, but still writes it and exits 0.
#[test]
fn ack_warns_on_stderr_for_unknown_id() {
    // Given an empty temp project root.
    let root = TempDir::new().expect("tempdir");
    let id = "Totally-Made-Up-Id-XYZ";

    // When acknowledging an id that is neither in LICENSES/ nor the corpus.
    let (code, _stdout, stderr) = run_in(&root, &["license", "ack", id]);

    // Then a warning naming the id is on stderr, and the command still succeeds.
    assert_eq!(code, 0);
    assert!(
        stderr.contains("warning"),
        "expected `warning` on stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains(id),
        "expected the unknown id {id:?} in stderr, got: {stderr:?}"
    );
    // And the id was written anyway (fail-open).
    assert!(
        root.path().join("auditah.toml").exists(),
        "expected auditah.toml to be written despite the warning"
    );
}

// Test case 4: `init` against an existing config without `--force` returns a
// technical error, which the dispatch layer maps to exit code 2.
#[test]
fn init_without_force_on_existing_file_exits_2() {
    // Given a temp project root that already has an auditah.toml.
    let root = TempDir::new().expect("tempdir");
    std::fs::write(root.path().join("auditah.toml"), "# pre-existing\n").expect("seed config");

    // When running `auditah init` without --force.
    let (code, _stdout, stderr) = run_in(&root, &["init"]);

    // Then the process exits 2 (technical failure -> Err(AppError)).
    assert_eq!(code, 2, "expected exit code 2, stderr was: {stderr:?}");
    assert!(
        stderr.contains("already exists"),
        "expected an `already exists` message, got: {stderr:?}"
    );
}
