//! Integration tests: error-scenario coverage.
//!
//! Covers the error paths of every `Result`-returning public fn:
//! - Content errors (malformed TOML, removed/stale fields) via bad file content.
//! - IO errors via `FakeFs` injection (`fail_write`/`fail_walk`) and real temptree
//!   structurally-unwritable paths.
//! - CLI `run()` semantics: clean→`Ok(Success)`, violations→`Ok(ComplianceFailure)`,
//!   technical failure→`Err`; plus exit-code mapping.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::cli::audit_cmd::{run as audit_run, AuditCmd};
use auditah::cli::command_to_exit_code;
use auditah::cli::generate_cmd::GenerateCmd;
use auditah::cli::license_provision_cmd::LicenseProvisionCmd;
use auditah::cli::CommandStatus;
use auditah::discovery::enumerator::ExcludeMatcher;
use auditah::discovery::resolver::resolve;
use auditah::registry::LicenseRegistry;
use auditah::services::clock::RealClock;
use auditah::services::fs::FsService;
use auditah::services::{ClockService, Services};
use auditah::test_support::FakeFs;
use error_stack::Report;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use temptree::temptree;

mod common;

/// Real clock for tests that don't care about the year default.
fn real_clock() -> ClockService {
    ClockService::new(Arc::new(RealClock::new()))
}

// ---------------------------------------------------------------------------
// LicenseRegistry::load — content errors
// ---------------------------------------------------------------------------

#[test]
fn registry_load_rejects_malformed_project_local_toml() {
    // Given a project with a malformed LICENSES/*.toml.
    let tree = temptree! {
        "LICENSES": {
            "Bad.toml": "this is not valid toml = =",
        }
    };
    let fs = common::real_fs();
    let root = tree.path();

    // When loading the registry.
    let result = LicenseRegistry::load(&fs, root);

    // Then it errors (malformed TOML rejected).
    assert!(result.is_err(), "malformed LICENSES/*.toml must error");
}

#[test]
fn registry_load_rejects_dropped_text_field() {
    // Given a LICENSES/*.toml carrying the removed inline `text` field.
    // (The text store is now LICENSES/<id>.txt; the grid schema no longer has `text`.)
    let tree = temptree! {
        "LICENSES": {
            "LicenseRef-Text.toml": r#"
id = "LicenseRef-Text"
name = "Text"
url = "https://example.com"
text = "should be rejected"

[terms]
requires_attribution = true
requires_license_notice = false
requires_source_disclosure = false
derivatives = "allowed"
requires_modification_notice = false
allows_commercial_use = true
allows_redistribution = true
manual_review = false
"#,
        }
    };
    let fs = common::real_fs();
    let root = tree.path();

    // When loading the registry.
    let result = LicenseRegistry::load(&fs, root);

    // Then it errors (deny_unknown_fields rejects the removed field).
    assert!(
        result.is_err(),
        "LicenseRef TOML with removed `text` field must error"
    );
}

// ---------------------------------------------------------------------------
// ExcludeMatcher::new — invalid glob
// ---------------------------------------------------------------------------

#[test]
fn exclude_matcher_rejects_invalid_glob_pattern() {
    // Given an invalid glob pattern (unbalanced brackets).
    let bad = vec!["**/[invalid".to_string()];

    // When constructing the matcher.
    let result = ExcludeMatcher::new(&bad);

    // Then it errors (invalid glob rejected).
    assert!(result.is_err(), "invalid glob pattern must error");
}

// ---------------------------------------------------------------------------
// resolve — content errors
// ---------------------------------------------------------------------------

#[test]
fn resolve_errors_on_malformed_sidecar_toml() {
    // Given an asset with a malformed sidecar.
    let fs = FsService::new(Arc::new(FakeFs::with_files([
        (Path::new("/x.glb"), ""),
        (Path::new("/x.glb.attr.toml"), "not valid toml = ="),
    ])));

    let services = Services::test().fs(fs).build();

    // When resolving.
    let result = resolve(&services, Path::new("/x.glb"), Path::new("/"));

    // Then it errors (malformed TOML rejected).
    assert!(result.is_err(), "malformed sidecar TOML must error");
}

#[test]
fn resolve_errors_on_sidecar_missing_license_field() {
    // Given a sidecar that parses but is missing the required `license` field.
    let fs = FsService::new(Arc::new(FakeFs::with_files([
        (Path::new("/x.glb"), ""),
        (
            Path::new("/x.glb.attr.toml"),
            r#"title = "X"
author = "A"
year = 2020
source = "https://example.com"
"#,
        ),
    ])));

    let services = Services::test().fs(fs).build();

    // When resolving.
    let result = resolve(&services, Path::new("/x.glb"), Path::new("/"));

    // Then it errors (missing required field rejected).
    assert!(
        result.is_err(),
        "sidecar missing `license` field must error"
    );
}

#[test]
fn resolve_errors_on_sidecar_with_invalid_derivatives_value() {
    // Given a sidecar with an invalid derivatives enum value under [overrides].
    let fs = FsService::new(Arc::new(FakeFs::with_files([
        (Path::new("/x.glb"), ""),
        (
            Path::new("/x.glb.attr.toml"),
            r#"title = "X"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"

[overrides]
derivatives = "foobar"
"#,
        ),
    ])));

    let services = Services::test().fs(fs).build();

    // When resolving.
    let result = resolve(&services, Path::new("/x.glb"), Path::new("/"));

    // Then it errors (invalid enum value rejected).
    assert!(
        result.is_err(),
        "sidecar with invalid derivatives value must error"
    );
}

#[test]
fn resolve_errors_on_sidecar_with_unknown_override_field() {
    // Given a sidecar with a stale (removed) override field — proves
    // deny_unknown_fields on Overrides holds through the resolver parse path.
    let fs = FsService::new(Arc::new(FakeFs::with_files([
        (Path::new("/x.glb"), ""),
        (
            Path::new("/x.glb.attr.toml"),
            r#"title = "X"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"

[overrides]
allows_modifications = true
"#,
        ),
    ])));

    let services = Services::test().fs(fs).build();

    // When resolving.
    let result = resolve(&services, Path::new("/x.glb"), Path::new("/"));

    // Then it errors (unknown override field rejected by deny_unknown_fields).
    assert!(
        result.is_err(),
        "sidecar with unknown override field must error"
    );
}

// ---------------------------------------------------------------------------
// IO errors via FakeFs injection
// ---------------------------------------------------------------------------

#[test]
fn write_sidecar_errors_on_injected_write_failure() {
    // Given a Services whose FakeFs is set to fail writes to the sidecar path.
    let fs = FsService::new(Arc::new(
        FakeFs::default().fail_write(Path::new("/x.glb.attr.toml")),
    ));
    let _registry = LicenseRegistry::empty();
    let services = Services::test().fs(fs).clock(real_clock()).build();
    let rec = common::record("LicenseRef-Asset");

    // When writing the sidecar.
    let result = auditah::add::write_sidecar(&services, Path::new("/x.glb"), &rec);

    // Then it errors (write failure propagated).
    assert!(
        result.is_err(),
        "write_sidecar must propagate write failure"
    );
}

#[test]
fn generate_credits_errors_on_injected_write_failure() {
    // Given a credits ctx whose FakeFs is set to fail writes to the output path.
    use auditah::config::Config;
    use auditah::credits::generate_credits;
    use auditah::services::config::ConfigService;
    let fs = FsService::new(Arc::new(
        FakeFs::default().fail_write(Path::new("/out/CREDITS.md")),
    ));
    let services = {
        let cfg = Config {
            commercial_project: false,
            redistributes_assets: false,
            manual_review_acknowledged: Vec::new(),
            exclude: Vec::new(),
        };
        Services::test()
            .fs(fs)
            .clock(real_clock())
            .config(ConfigService::new(Arc::from(Path::new("/")), Arc::new(cfg)))
            .build()
    };

    // When generating credits to the failing output path.
    let result = generate_credits(&services, Path::new("/out/CREDITS.md"));

    // Then it errors (write failure propagated).
    assert!(
        result.is_err(),
        "generate_credits must propagate write failure"
    );
}

// ---------------------------------------------------------------------------
// IO errors via temptree (real RealFs, structurally-unwritable path)
// ---------------------------------------------------------------------------

#[test]
fn write_sidecar_errors_when_target_is_under_a_file() {
    // Given a path where a regular file occupies what should be a parent directory.
    let tree = temptree! {
        "blocker": "i am a file, not a dir"
    };
    let root = tree.path();
    let fs = common::real_fs();
    let services = Services::test().fs(fs).clock(real_clock()).build();
    let rec = common::record("LicenseRef-Asset");
    // Writing to blocker/x.glb.attr.toml fails because `blocker` is a file.
    let target = root.join("blocker").join("x.glb");

    // When writing the sidecar under the file path.
    let result = auditah::add::write_sidecar(&services, &target, &rec);

    // Then it errors (cannot write under a file).
    assert!(result.is_err(), "write under a file must error");
}

// ---------------------------------------------------------------------------
// run_audit — walk failure propagation
// ---------------------------------------------------------------------------

#[test]
fn run_audit_propagates_directory_listing_failure() {
    // Given an audit over a FakeFs configured to fail the root directory listing.
    use auditah::audit::run_audit;
    use auditah::config::Config;
    use auditah::services::config::ConfigService;
    let fs = FsService::new(Arc::new(
        FakeFs::default().fail_list_dir(Path::new("/proj")),
    ));
    let services = {
        let cfg = Config {
            commercial_project: false,
            redistributes_assets: false,
            manual_review_acknowledged: Vec::new(),
            exclude: Vec::new(),
        };
        Services::test()
            .fs(fs)
            .clock(real_clock())
            .config(ConfigService::new(
                Arc::from(Path::new("/proj")),
                Arc::new(cfg),
            ))
            .build()
    };

    // When running the audit.
    let result = run_audit(&services);

    // Then the listing failure propagates as an error.
    assert!(
        result.is_err(),
        "directory listing failure must propagate as audit error"
    );
}

#[test]
fn run_audit_propagates_build_excludes_error_without_panic() {
    // Given a directly-constructed Config (bypassing load) with an invalid
    // exclude glob — proves build_excludes is fallible, not a panic.
    use auditah::audit::run_audit;
    use auditah::config::Config;
    use auditah::services::config::ConfigService;
    let fs = FsService::new(Arc::new(FakeFs::default()));
    let services = {
        let cfg = Config {
            commercial_project: false,
            redistributes_assets: false,
            manual_review_acknowledged: Vec::new(),
            exclude: vec!["**/[invalid".to_string()],
        };
        Services::test()
            .fs(fs)
            .clock(real_clock())
            .config(ConfigService::new(
                Arc::from(Path::new("/proj")),
                Arc::new(cfg),
            ))
            .build()
    };

    // When running the audit.
    let result = run_audit(&services);

    // Then the invalid-glob error propagates as Err (never panics).
    assert!(
        result.is_err(),
        "build_excludes failure must propagate as audit error, not panic"
    );
}

// ---------------------------------------------------------------------------
// CLI run() semantics + exit-code mapping
// ---------------------------------------------------------------------------

#[test]
fn audit_cmd_clean_project_returns_ok_success() {
    // Given a clean project (a LicenseRef-Asset asset with sidecar + LICENSES text).
    let tree = temptree! {
        "rock.glb": "binary",
        "rock.glb.attr.toml": r#"
title = "Rock"
author = "A"
year = 2020
license = "LicenseRef-Asset"
source = "https://example.com"
"#,
    };
    let root = tree.path();
    common::seed_license(root, "LicenseRef-Asset");
    let cmd = AuditCmd {
        root: root.to_path_buf(),
        ..Default::default()
    };

    // When running the audit command.
    let result = audit_run(&common::resolve_services(root, &cmd.root), &cmd);

    // Then it returns Ok(Success) (clean project).
    let status = result.expect("clean audit should be Ok");
    assert_eq!(status, CommandStatus::Success);
    assert_eq!(command_to_exit_code(&Ok(status)), 0);
}

#[test]
fn audit_cmd_violations_returns_ok_compliance_failure() {
    // Given a project with an uncovered asset (no sidecar).
    let tree = temptree! {
        "sword.glb": "binary",
    };
    let root = tree.path();
    // init creates LICENSES/; discovery requires it to resolve the project root.
    std::fs::create_dir_all(root.join("LICENSES")).expect("mkdir LICENSES");
    // No license seeded on purpose — an uncovered asset fails regardless.
    let cmd = AuditCmd {
        root: root.to_path_buf(),
        ..Default::default()
    };

    // When running the audit command.
    let result = audit_run(&common::resolve_services(root, &cmd.root), &cmd);

    // Then it returns Ok(ComplianceFailure) (violations found, exit 1).
    let status = result.expect("audit with violations should be Ok");
    assert_eq!(status, CommandStatus::ComplianceFailure);
    assert_eq!(command_to_exit_code(&Ok(status)), 1);
}

#[test]
fn license_cmd_run_returns_err_on_write_failure() {
    // Given a license command whose file target sits under an existing file
    // (unwritable).
    use auditah::cli::license_assign_cmd::{run as license_run, LicenseAssignCmd};
    let tree = temptree! {
        "blocker": "i am a file, not a dir",
        "LICENSES": {},
    };
    let root = tree.path();
    let target = root.join("blocker").join("x.glb");
    let cmd = LicenseAssignCmd {
        target: target.clone(),
        id: "LicenseRef-Asset".to_string(),
        author: "A".to_string(),
        title: Some("X".to_string()),
        year: Some(2020),
        source: Some("https://example.com".to_string()),
        modified: false,
        root: Some(root.to_path_buf()),
    };

    // When running the license command (root resolved explicitly so the
    // failure under test is the write step, not root discovery).
    let result = license_run(&common::real_services(root), &cmd);

    // Then it returns Err (the target's parent is a file, so metadata/write fails).
    assert!(result.is_err(), "license write failure must return Err");
}

#[test]
fn audit_cmd_missing_root_returns_err_exit_two() {
    // Given a root path that does not exist.
    let cmd = AuditCmd {
        root: PathBuf::from("/nonexistent/auditah/path/xyz"),
        ..Default::default()
    };

    // When dispatching against a missing root: resolve_or_error fails before any
    // Services is built (no LICENSES/ ancestor to anchor on).
    let result = auditah::project::resolve_or_error(std::path::Path::new("."), &cmd.root);

    assert!(result.is_err(), "missing root must be a technical failure");
    assert_eq!(
        command_to_exit_code(&result.map(|_| CommandStatus::Success)),
        2
    );
}

#[rstest::rstest]
#[case::success(Ok(CommandStatus::Success), 0)]
#[case::compliance_failure(Ok(CommandStatus::ComplianceFailure), 1)]
#[case::error(Err(Report::from(auditah::AppError)), 2)]
fn command_to_exit_code_maps_outcome_to_code(
    #[case] outcome: Result<CommandStatus, Report<auditah::AppError>>,
    #[case] expected: i32,
) {
    // Given a command outcome (Success, ComplianceFailure, or Err).
    // When mapping to an exit code.
    let code = command_to_exit_code(&outcome);

    // Then the outcome maps to the expected process exit code.
    assert_eq!(code, expected);
}

// audit hard-errors when no ancestor LICENSES/ exists (no fallback to --root).
#[test]
fn audit_cmd_no_licenses_dir_returns_err() {
    // Given a project root with no LICENSES/ directory anywhere up the tree.
    let tree = temptree! {
        "sword.glb": "binary",
    };
    let root = tree.path();
    let cmd = AuditCmd {
        root: root.to_path_buf(),
        ..Default::default()
    };

    // When dispatching audit with no LICENSES/ ancestor: resolve_or_error hard-errors
    // (no Services is built).
    let result = auditah::project::resolve_or_error(root, &cmd.root);
    // Then it returns Err (discovery failure points the user at `auditah init`).
    assert!(
        result.is_err(),
        "audit must hard-error when no ancestor LICENSES/ exists"
    );
    let report = result.expect_err("err");
    let rendered = format!("{report:?}");
    assert!(
        rendered.contains("auditah init"),
        "error must mention `auditah init`, got: {rendered}"
    );
}

// generate hard-errors when no ancestor LICENSES/ exists.
#[test]
fn generate_cmd_no_licenses_dir_returns_err() {
    // Given a project root with no LICENSES/ directory anywhere up the tree.
    let tree = temptree! {
        "sword.glb": "binary",
    };
    let root = tree.path();
    let cmd = GenerateCmd {
        root: root.to_path_buf(),
        output_credits: None,
        output_notices: None,
        output_bom: None,
    };

    // When dispatching generate with no LICENSES/ ancestor: resolve_or_error hard-errors.
    let result = auditah::project::resolve_or_error(root, &cmd.root);
    // Then it returns Err pointing the user at `auditah init`.
    assert!(
        result.is_err(),
        "generate must hard-error when no ancestor LICENSES/ exists"
    );
    let rendered = format!("{:?}", result.expect_err("err"));
    assert!(
        rendered.contains("auditah init"),
        "error must mention `auditah init`, got: {rendered}"
    );
}

// license provision hard-errors when no LICENSES/ exists, and does not create one.
#[test]
fn license_cmd_no_licenses_dir_returns_err_and_does_not_create_licenses() {
    // Given a project root with no LICENSES/ directory anywhere up the tree.
    let tree = temptree! {
        "sword.glb": "binary",
    };
    let root = tree.path();
    let cmd = LicenseProvisionCmd {
        name: "MIT".to_string(),
        custom: false,
        root: root.to_path_buf(),
    };

    // When dispatching license provision with no LICENSES/ ancestor: resolve_or_error hard-errors.
    let result = auditah::project::resolve_or_error(root, &cmd.root);
    // Then it returns Err pointing the user at `auditah init`.
    assert!(
        result.is_err(),
        "license provision must hard-error when no LICENSES/ exists"
    );
    let rendered = format!("{:?}", result.expect_err("err"));
    assert!(
        rendered.contains("auditah init"),
        "error must mention `auditah init`, got: {rendered}"
    );
    // And it must NOT have created LICENSES/ (only `init` creates it).
    assert!(
        !root.join("LICENSES").exists(),
        "license provision must not bootstrap a LICENSES/ directory"
    );
}
