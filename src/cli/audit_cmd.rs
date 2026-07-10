//! `auditah audit` — obligation-aware compliance check. Stub until Phase 4.

use clap::Args;

/// Audit license compliance of assets under the project.
#[derive(Debug, Args)]
pub struct AuditCmd {
    /// Project root to audit (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: std::path::PathBuf,
}

/// Run the audit command.
///
/// Stub; real implementation lands in Phase 4.
pub fn run(_cmd: &AuditCmd) -> i32 {
    eprintln!("audit: not yet implemented (Phase 4)");
    0
}
