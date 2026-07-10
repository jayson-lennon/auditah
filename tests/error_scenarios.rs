//! Integration tests: error-scenario coverage.
//!
//! Covers the error paths of every `Result`-returning public fn:
//! - Content errors (malformed TOML, empty text, missing fields) via bad file content.
//! - IO errors via `FakeFs` injection (`fail_write`/`fail_walk`) and real temptree
//!   structurally-unwritable paths.
//! - CLI `run()` semantics: clean→`Ok(Success)`, violations→`Ok(ComplianceFailure)`,
//!   technical failure→`Err`; plus exit-code mapping.

use auditah::cli::audit_cmd::{run as audit_run, AuditCmd};
use auditah::cli::command_to_exit_code;
use auditah::cli::CommandStatus;
use auditah::discovery::enumerator::ExcludeMatcher;
use auditah::discovery::resolver::resolve;
use auditah::registry::LicenseRegistry;
use auditah::services::fs::FsService;
use auditah::services::Services;
use auditah::test_support::FakeFs;
use error_stack::Report;
use std::path::Path;
use std::sync::Arc;
use temptree::temptree;

mod common;

// ---------------------------------------------------------------------------
// LicenseRegistry::load — content errors
// ---------------------------------------------------------------------------

#[test]
fn registry_load_rejects_malformed_project_local_toml() {
    // Given a project with a malformed licenses/*.toml.
    let tree = temptree! {
        "licenses": {
            "Bad.toml": "this is not valid toml = =",
        }
    };
    let fs = FsService::new(Arc::new(RealFs::new()));
    let root = tree.path();

    // When loading the registry.
    let result = LicenseRegistry::load(&fs, root);

    // Then it errors (malformed TOML rejected).
    assert!(result.is_err(), "malformed licenses/*.toml must error");
}

#[test]
fn registry_load_rejects_licenseref_with_empty_text() {
    // Given a project with a LicenseRef-* whose inline text is empty.
    let tree = temptree! {
        "licenses": {
            "LicenseRef-Empty.toml": r#"
id = "LicenseRef-Empty"
name = "Empty"
url = "https://example.com"
text = ""
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
    let fs = FsService::new(Arc::new(RealFs::new()));
    let root = tree.path();

    // When loading the registry.
    let result = LicenseRegistry::load(&fs, root);

    // Then it errors (custom license requires non-empty text).
    assert!(result.is_err(), "LicenseRef with empty text must error");
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

    // When resolving.
    let result = resolve(&fs, Path::new("/x.glb"), Path::new("/"));

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

    // When resolving.
    let result = resolve(&fs, Path::new("/x.glb"), Path::new("/"));

    // Then it errors (missing required field rejected).
    assert!(
        result.is_err(),
        "sidecar missing `license` field must error"
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
    let registry = LicenseRegistry::embedded_only();
    let services = Services::from_parts(fs, registry);
    let rec = common::record("CC0-1.0");

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
    use auditah::credits::{generate_credits, CreditsCtx};
    let fs = FsService::new(Arc::new(
        FakeFs::default().fail_write(Path::new("/out/CREDITS.md")),
    ));
    let registry = LicenseRegistry::embedded_only();
    let services = Services::from_parts(fs, registry);
    let cfg = Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    };
    let ctx = CreditsCtx {
        services: &services,
        config: &cfg,
        root: Path::new("/"),
    };

    // When generating credits to the failing output path.
    let result = generate_credits(&ctx, Path::new("/out/CREDITS.md"));

    // Then it errors (write failure propagated).
    assert!(
        result.is_err(),
        "generate_credits must propagate write failure"
    );
}

#[test]
fn init_licenses_errors_on_injected_write_failure() {
    // Given a Services whose FakeFs is set to fail writes into LICENSES/.
    use auditah::init_licenses::init_licenses;
    let fs = FsService::new(Arc::new(
        FakeFs::default().fail_write(Path::new("/proj/LICENSES/CC0-1.0.txt")),
    ));
    let registry = LicenseRegistry::embedded_only();
    let services = Services::from_parts(fs, registry);

    // When running init-licenses.
    let result = init_licenses(&services, Path::new("/proj"));

    // Then it errors (write failure propagated).
    assert!(
        result.is_err(),
        "init_licenses must propagate write failure"
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
    let registry = LicenseRegistry::embedded_only();
    let services = Services::from_parts(fs, registry);
    let rec = common::record("CC0-1.0");
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
fn run_audit_propagates_walk_failure() {
    // Given an audit over a FakeFs configured to fail the walk.
    use auditah::audit::{run_audit, AuditCtx};
    use auditah::config::Config;
    let fs = FsService::new(Arc::new(FakeFs::default().fail_walk(Path::new("/proj"))));
    let registry = LicenseRegistry::embedded_only();
    let services = Services::from_parts(fs, registry);
    let cfg = Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    };
    let ctx = AuditCtx {
        services: &services,
        config: &cfg,
        root: Path::new("/proj"),
    };

    // When running the audit.
    let result = run_audit(&ctx);

    // Then the walk failure propagates as an error.
    assert!(
        result.is_err(),
        "walk failure must propagate as audit error"
    );
}

// ---------------------------------------------------------------------------
// CLI run() semantics + exit-code mapping
// ---------------------------------------------------------------------------

#[test]
fn audit_cmd_clean_project_returns_ok_success() {
    // Given a clean project (CC0 asset with sidecar + LICENSES text).
    let tree = temptree! {
        "rock.glb": "binary",
        "rock.glb.attr.toml": r#"
title = "Rock"
author = "A"
year = 2020
license = "CC0-1.0"
source = "https://example.com"
"#,
    };
    let root = tree.path();
    common::seed_licenses(root);
    let cmd = AuditCmd {
        root: root.to_path_buf(),
    };

    // When running the audit command.
    let result = audit_run(&cmd);

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
    common::seed_licenses(root);
    let cmd = AuditCmd {
        root: root.to_path_buf(),
    };

    // When running the audit command.
    let result = audit_run(&cmd);

    // Then it returns Ok(ComplianceFailure) (violations found, exit 1).
    let status = result.expect("audit with violations should be Ok");
    assert_eq!(status, CommandStatus::ComplianceFailure);
    assert_eq!(command_to_exit_code(&Ok(status)), 1);
}

#[test]
fn add_cmd_run_returns_err_on_write_failure() {
    // Given an add command whose target path is under an existing file (unwritable).
    use auditah::cli::add_cmd::{run as add_run, AddCmd};
    let tree = temptree! {
        "blocker": "i am a file, not a dir"
    };
    let root = tree.path();
    let target = root.join("blocker").join("x.glb");
    let cmd = AddCmd {
        file: target,
        title: Some("X".to_string()),
        author: Some("A".to_string()),
        year: Some(2020),
        license: Some("CC0-1.0".to_string()),
        source: Some("https://example.com".to_string()),
        modified: false,
    };

    // When running the add command.
    let result = add_run(&cmd);

    // Then it returns Err (write failure, exit 2).
    assert!(result.is_err(), "add write failure must return Err");
    assert_eq!(command_to_exit_code(&result), 2);
}

#[test]
fn audit_cmd_missing_root_returns_err_exit_two() {
    // Given a root path that does not exist.
    let cmd = AuditCmd {
        root: std::path::PathBuf::from("/nonexistent/auditah/path/xyz"),
    };

    // When running the audit command.
    let result = audit_run(&cmd);

    // Then it returns Err (technical failure, exit 2).
    assert!(result.is_err(), "missing root must be a technical failure");
    assert_eq!(command_to_exit_code(&result), 2);
}

#[test]
fn command_to_exit_code_maps_all_three_outcomes() {
    // Given the three possible command outcomes.
    let ok_success: Result<CommandStatus, Report<auditah::AppError>> = Ok(CommandStatus::Success);
    let ok_fail: Result<CommandStatus, Report<auditah::AppError>> =
        Ok(CommandStatus::ComplianceFailure);
    let err: Result<CommandStatus, Report<auditah::AppError>> =
        Err(Report::from(auditah::AppError));

    // When mapping to exit codes.
    // Then Success→0, ComplianceFailure→1, Err→2.
    assert_eq!(command_to_exit_code(&ok_success), 0);
    assert_eq!(command_to_exit_code(&ok_fail), 1);
    assert_eq!(command_to_exit_code(&err), 2);
}

// ---------------------------------------------------------------------------
// Re-export so the `RealFs` name resolves in the registry tests above.
// ---------------------------------------------------------------------------

use auditah::services::fs::RealFs;
