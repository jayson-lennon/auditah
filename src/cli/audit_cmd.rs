//! `auditah audit` — obligation-aware compliance check.

use std::path::PathBuf;

use clap::Args;

use auditah::audit::{run_audit, AuditCtx};
use auditah::config::Config;
use auditah::services::Services;

/// Audit license compliance of assets under the project.
#[derive(Debug, Args)]
pub struct AuditCmd {
    /// Project root to audit (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Run the audit command. Returns the process exit code (0 = clean, 1 = Fail
/// findings present, 2 = pipeline error).
pub fn run(cmd: &AuditCmd) -> i32 {
    let root = &cmd.root;
    let services = Services::real();
    let config = match Config::load(&services.fs, root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to load config: {e:?}");
            return 2;
        }
    };
    let ctx = AuditCtx {
        services: &services,
        config: &config,
        root,
    };
    let report = match run_audit(&ctx) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: audit pipeline failed: {e:?}");
            return 2;
        }
    };
    render_report(&report);
    if report.has_failures() {
        1
    } else {
        0
    }
}

/// Render the report grouped by severity: FAILs first, then FLAGs.
fn render_report(report: &auditah::audit::report::AuditReport) {
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
