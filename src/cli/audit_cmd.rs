//! `auditah audit` — obligation-aware compliance check.

use std::path::PathBuf;

use crate::audit::report::AuditReport;
use crate::AppError;
use clap::Args;

use crate::audit::{run_audit, AuditCtx};
use crate::config::Config;
use crate::services::Services;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Audit license compliance of assets under the project.
#[derive(Debug, Args)]
pub struct AuditCmd {
    /// Project root to audit (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Run the audit command.
///
/// Returns `Ok(Success)` when the project is clean, `Ok(ComplianceFailure)`
/// when FAIL findings are present (exit 1), and `Err` on technical failures
/// (exit 2).
///
/// # Errors
///
/// Returns an error if the services, config, or audit pipeline fail.
pub fn run(cmd: &AuditCmd) -> Result<CommandStatus, Report<AppError>> {
    let root = &cmd.root;
    let services = Services::real().change_context(AppError)?;
    let config = Config::load(&services.fs, root)
        .change_context(AppError)
        .attach("failed to load config")?;
    let ctx = AuditCtx {
        services: &services,
        config: &config,
        root,
    };
    let report = run_audit(&ctx)
        .change_context(AppError)
        .attach("audit pipeline failed")?;
    render_report(&report);
    if report.has_failures() {
        Ok(CommandStatus::ComplianceFailure)
    } else {
        Ok(CommandStatus::Success)
    }
}

/// Render the report grouped by severity: FAILs first, then FLAGs.
fn render_report(report: &AuditReport) {
    use crate::audit::report::Severity;
    if report.findings.is_empty() {
        println!("audit: clean — no findings");
        return;
    }
    let fails: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Fail)
        .collect();
    let flags: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Flag)
        .collect();

    if !fails.is_empty() {
        println!("FAIL ({}):", fails.len());
        for f in &fails {
            println!("  {} — {}", f.asset.display(), f.detail);
        }
    }
    if !flags.is_empty() {
        println!("FLAG ({}):", flags.len());
        for f in &flags {
            println!("  {} — {}", f.asset.display(), f.detail);
        }
    }
}
