//! CLI parse-time behavior for the `auditah license assign` (and `assign`) command.
//!
//! `--id` and `--author` are required at parse time (non-`Option` clap fields),
//! so omitting either is a parse failure with a non-zero exit before `run()`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;

// A `license assign` invocation missing `--id` fails at parse time (non-zero exit),
// before any filesystem work happens.
#[test]
fn license_missing_id_fails_at_parse_time() {
    // Given no project setup — parse failure happens before run().
    // When invoking `license assign` without `--id`.
    let output = Command::cargo_bin("auditah")
        .expect("auditah binary built")
        .args([
            "license",
            "assign",
            "/tmp/does-not-matter.glb",
            "--author",
            "A",
        ])
        .output()
        .expect("run");

    // Then the command exits non-zero and stderr mentions the required `--id`.
    assert!(!output.status.success(), "missing --id must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        predicate::str::contains("--id <ID>").eval(&stderr)
            || predicate::str::contains("--id").eval(&stderr),
        "stderr should mention the missing --id flag: {stderr}"
    );
}

// A `license assign` invocation missing `--author` fails at parse time (non-zero exit),
// before any filesystem work happens.
#[test]
fn license_missing_author_fails_at_parse_time() {
    // Given no project setup — parse failure happens before run().
    // When invoking `license assign` without `--author`.
    let output = Command::cargo_bin("auditah")
        .expect("auditah binary built")
        .args([
            "license",
            "assign",
            "/tmp/does-not-matter.glb",
            "--id",
            "MIT",
        ])
        .output()
        .expect("run");

    // Then the command exits non-zero and stderr mentions the required `--author`.
    assert!(!output.status.success(), "missing --author must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        predicate::str::contains("--author <AUTHOR>").eval(&stderr)
            || predicate::str::contains("--author").eval(&stderr),
        "stderr should mention the missing --author flag: {stderr}"
    );
}
