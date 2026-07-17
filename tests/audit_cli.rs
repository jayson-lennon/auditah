//! End-to-end CLI coverage for `auditah audit`: spawns the real binary over
//! `temptree`/`tempfile` project roots and asserts on captured stdout, stderr,
//! and exit code. These verify the *CLI contract* (flag wiring, exit codes,
//! output routing) that the kernel-level tests cannot reach.
//!
//! Case mapping to the approved plan's test table:
//! - Case 9  (`accepted_paths_shown_only_when_verbose`)
//! - Case 11 (`errors_printed_after_summary`)
//! - Case 12 (`exit_codes_*`)
//!
//! Case 10 (progress on stderr) is TTY-gated in production and is therefore
//! verified at the pipeline layer (`progress_channel_emits_one_tick_per_asset`),
//! not here — see the note at the bottom of this file.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_raw_string_hashes
)]

mod common;

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use temptree::temptree;

/// Run `auditah audit` against `root` with the given extra args. Returns the
/// completed command for stdout/stderr/exit-code assertions.
fn audit(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("auditah").expect("auditah binary built");
    cmd.arg("audit").arg("--root").arg(root);
    cmd
}

/// Seed a permissive license on disk so the binary's `LicenseRegistry::load`
/// resolves it and the `LICENSES/<id>.txt` presence check passes.
fn seed_license(root: &Path, id: &str) {
    common::seed_license(root, id);
}

// ---------------------------------------------------------------------------
// Case 9: ACCEPTED paths shown only under --verbose
// ---------------------------------------------------------------------------

#[test]
fn accepted_paths_hidden_without_verbose() {
    // Given a clean, fully-licensed asset at the project root.
    let tree = temptree! {
        "_manifest.toml": r##"
title = "Sample"
author = "Artist"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com"
"##,
        "sword.glb": "binary",
    };
    seed_license(tree.path(), "LicenseRef-Mit");

    // When auditing quietly (default).
    let output = audit(tree.path()).output().expect("run");

    // Then stdout has no ACCEPTED block — passes are silent by default.
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("ACCEPTED"));
}

#[test]
fn accepted_paths_shown_with_verbose() {
    // Given a clean, fully-licensed asset.
    let tree = temptree! {
        "_manifest.toml": r##"
title = "Sample"
author = "Artist"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com"
"##,
        "sword.glb": "binary",
    };
    seed_license(tree.path(), "LicenseRef-Mit");

    // When auditing verbosely.
    let output = audit(tree.path()).arg("--verbose").output().expect("run");

    // Then stdout lists the accepted asset path.
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ACCEPTED (1):"), "stdout was: {stdout}");
    assert!(stdout.contains("sword.glb"));
}

// ---------------------------------------------------------------------------
// Case 12: exit codes 0 / 1 / 2
// ---------------------------------------------------------------------------

#[test]
fn clean_project_exits_zero() {
    // Given a fully-compliant project.
    let tree = temptree! {
        "_manifest.toml": r##"
title = "Sample"
author = "Artist"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com"
"##,
        "sword.glb": "binary",
    };
    seed_license(tree.path(), "LicenseRef-Mit");

    // When auditing.
    // Then the process exits 0 (Success).
    audit(tree.path()).assert().success();
}

#[test]
fn compliance_failure_exits_one() {
    // Given an unlicensed asset (no manifest reaches it, no sidecar).
    let tree = temptree! {
        "orphan.glb": "binary",
    };

    // When auditing.
    // Then the process exits 1 (ComplianceFailure), not 0 or 2.
    audit(tree.path())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("FAIL"));
}

#[test]
fn technical_error_exits_two() {
    // Given a directory whose _manifest.toml is unparseable — a technical
    // (infrastructure) failure distinct from a compliance finding.
    let tree = temptree! {
        "_manifest.toml": "this is not = valid toml {{{",
        "asset.glb": "binary",
    };

    // When auditing.
    // Then the process exits 2 (Error), which is more severe than a
    // compliance failure's exit 1 — technical faults must not be conflated.
    audit(tree.path()).assert().failure().code(2);
}

// ---------------------------------------------------------------------------
// Case 11: technical errors printed dead last, after the summary
// ---------------------------------------------------------------------------

#[test]
fn errors_go_to_stderr_after_summary_on_stdout() {
    // Given a project with both a compliance failure AND a technical error:
    // an unlicensed asset at the root, plus a subdir whose manifest is broken.
    let tree = temptree! {
        "unlicensed.glb": "binary",
        "broken": {
            "_manifest.toml": "this is not = valid toml {{{",
            "inner.glb": "binary",
        },
    };

    // When auditing.
    let output = audit(tree.path()).output().expect("run");

    // Then the exit code is 2 (technical error wins severity over the
    // compliance failure).
    assert_eq!(output.status.code(), Some(2));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // And the compliance summary lives on stdout (including the FAIL line for
    // the unlicensed root asset)…
    assert!(stdout.contains("summary:"));
    assert!(stdout.contains("FAIL"));
    assert!(
        !stdout.contains("ERRORS"),
        "errors must not leak to stdout: {stdout}"
    );

    // …while the technical error block lives on stderr, distinct and never
    // lost in compliance-finding noise.
    assert!(stderr.contains("ERRORS"), "stderr was: {stderr}");

    // The root's unlicensed asset IS reported as a compliance FAIL…
    assert!(stdout.contains("unlicensed.glb"));
    // …while the broken subtree's asset is NOT (it was skipped, not audited).
    assert!(!stdout.contains("inner.glb"));
}

// ---------------------------------------------------------------------------
// Note on Case 10 (progress streaming on stderr)
// ---------------------------------------------------------------------------
//
// The CLI's live-progress output is gated behind `std::io::stderr().is_terminal()`
// (src/cli/audit_cmd.rs). When a subprocess is spawned with captured pipes — as
// `assert_cmd` does, and as CI/logs/file-redirection do — stderr is NOT a TTY,
// so no progress lines are emitted. This is standard Unix tool behavior (color
// and progress bars are TTY-only), and it correctly addresses the original bug
// report: a human running `auditah audit` interactively now sees streaming
// progress instead of a long silence.
//
// The *streaming contract itself* (one progress tick per audited asset, emitted
// during the run rather than buffered) is therefore verified at the pipeline
// layer in tests/audit_pipeline_async.rs::progress_channel_emits_one_tick_per_asset,
// which is independent of TTY presence. Asserting on the TTY-gated presentation
// here would require forcing a pseudo-TTY, which is fragile and tests a
// condition that does not hold in the realistic piped scenario anyway.
