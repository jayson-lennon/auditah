//! `auditah init-pack` — write a directory manifest.toml. Stub until Phase 6.

use clap::Args;

/// Write a `manifest.toml` covering a directory + its subdirs.
#[derive(Debug, Args)]
pub struct InitPackCmd {
    /// Directory to cover with a manifest.
    pub dir: std::path::PathBuf,

    /// SPDX license ID (e.g. CC0-1.0, CC-BY-3.0).
    #[arg(long)]
    pub license: String,

    /// Author / copyright holder.
    #[arg(long)]
    pub author: String,
}

/// Run the init-pack command.
///
/// Stub; real implementation lands in Phase 6.
pub fn run(_cmd: &InitPackCmd) -> i32 {
    eprintln!("init-pack: not yet implemented (Phase 6)");
    0
}
