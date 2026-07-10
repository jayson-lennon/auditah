//! `auditah credits` — generate CREDITS.md. Stub until Phase 5.

use clap::Args;

/// Generate a CREDITS.md from attribution sidecars/manifests.
#[derive(Debug, Args)]
pub struct CreditsCmd {
    /// Project root to scan (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: std::path::PathBuf,

    /// Output file path (defaults to `<root>/CREDITS.md`).
    #[arg(long)]
    pub output: Option<std::path::PathBuf>,
}

/// Run the credits command.
///
/// Stub; real implementation lands in Phase 5.
pub fn run(_cmd: &CreditsCmd) -> i32 {
    eprintln!("credits: not yet implemented (Phase 5)");
    0
}
