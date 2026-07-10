//! `auditah audit` — obligation-aware compliance check.

use std::path::PathBuf;

use auditah::audit::report::AuditReport;
use auditah::AppError;
use clap::Args;

use auditah::audit::{run_audit, AuditCtx};
use auditah::config::Config;
use auditah::services::Services;
use error_stack::{Report, ResultExt};

/// Audit license compliance of assets under the project.
#[derive(Debug, Args)]
pub struct AuditCmd {
    /// Project root to audit (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Run the audit command. Returns the process exit code (0 = clean, 1 = Fail
/// findings present, 2 = pipeline error).
pub fn run(cmd: &AuditCmd) -> Result<(), Report<AppError>> {
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
    Ok(())
}

/// Render the report grouped by severity: FAILs first, then FLAGs.
fn render_report(report: &AuditReport) {
    use auditah::audit::report::Severity;
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
