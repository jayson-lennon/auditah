//! `auditah add` — scaffold a sidecar for a single asset. Stub until Phase 6.

use clap::Args;

/// Scaffold an `<asset>.attr.toml` sidecar for a single asset.
#[derive(Debug, Args)]
pub struct AddCmd {
    /// Path to the asset file to annotate.
    pub file: std::path::PathBuf,
}

/// Run the add command.
///
/// Stub; real implementation lands in Phase 6.
pub fn run(_cmd: &AddCmd) -> i32 {
    eprintln!("add: not yet implemented (Phase 6)");
    0
}
