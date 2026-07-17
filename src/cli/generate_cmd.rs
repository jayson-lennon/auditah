//! `auditah generate` — produce all distribution artifacts in one invocation.
//!
//! Runs the audit-gate first (no artifacts on a failing project), then calls
//! the three internal generators: CREDITS.md (attribution), NOTICES.md
//! (license text reproduction), and BOM.md (compliance obligations).

use std::path::PathBuf;

use crate::AppError;
use clap::Args;

use crate::audit::{run_audit, AuditCtx};
use crate::bom::{default_output_path as bom_default, generate_bom, BomCtx};
use crate::config::Config;
use crate::credits::{default_output_path as credits_default, generate_credits, CreditsCtx};
use crate::notices::{default_output_path as notices_default, generate_notices, NoticesCtx};
use crate::services::Services;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Generate all distribution artifacts: CREDITS.md, NOTICES.md, BOM.md.
#[derive(Debug, Args)]
pub struct GenerateCmd {
    /// Project root to scan (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// Output file for CREDITS.md (defaults to `<root>/CREDITS.md`).
    #[arg(long)]
    pub output_credits: Option<PathBuf>,

    /// Output file for NOTICES.md (defaults to `<root>/NOTICES.md`).
    #[arg(long)]
    pub output_notices: Option<PathBuf>,

    /// Output file for BOM.md (defaults to `<root>/BOM.md`).
    #[arg(long)]
    pub output_bom: Option<PathBuf>,
}

/// Run the generate command.
///
/// # Errors
///
/// Returns an error if services, config load, audit-gate, or any generator
/// fails.
pub fn run(cmd: &GenerateCmd) -> Result<CommandStatus, Report<AppError>> {
    let root = &cmd.root;
    let services = Services::real(root).change_context(AppError)?;
    let config = Config::load(&services.fs, root)
        .change_context(AppError)
        .attach("failed to load config")?;

    // Audit gate: no artifacts on a failing project.
    let audit_ctx = AuditCtx {
        services: &services,
        config: &config,
        root,
    };
    let report = run_audit(&audit_ctx).change_context(AppError)?;
    if report.has_failures() {
        return Err(Report::new(AppError)
            .attach(format!(
                "{} audit failure(s) — fix before generating artifacts",
                report.fail_count()
            ))
            .attach("run `auditah audit` for details"));
    }
    if report.has_errors() {
        return Err(Report::new(AppError)
            .attach(format!(
                "{} technical error(s) during audit — fix before generating artifacts",
                report.error_count()
            ))
            .attach("run `auditah audit` for details"));
    }

    let output_credits = cmd
        .output_credits
        .clone()
        .unwrap_or_else(|| credits_default(root));
    let output_notices = cmd
        .output_notices
        .clone()
        .unwrap_or_else(|| notices_default(root));
    let output_bom = cmd.output_bom.clone().unwrap_or_else(|| bom_default(root));

    let credits_ctx = CreditsCtx {
        services: &services,
        config: &config,
        root,
    };
    let notices_ctx = NoticesCtx {
        services: &services,
        config: &config,
        root,
    };
    let bom_ctx = BomCtx {
        services: &services,
        config: &config,
        root,
    };

    generate_credits(&credits_ctx, &output_credits)
        .change_context(AppError)
        .attach("failed to generate CREDITS.md")?;
    generate_notices(&notices_ctx, &output_notices)
        .change_context(AppError)
        .attach("failed to generate NOTICES.md")?;
    generate_bom(&bom_ctx, &output_bom)
        .change_context(AppError)
        .attach("failed to generate BOM.md")?;

    println!("generate: wrote {}", output_credits.display());
    println!("generate: wrote {}", output_notices.display());
    println!("generate: wrote {}", output_bom.display());
    Ok(CommandStatus::Success)
}
