//! `auditah credits` — generate a CREDITS.md from attribution data.

use std::path::PathBuf;

use crate::AppError;
use clap::Args;

use crate::config::Config;
use crate::credits::{default_output_path, generate_credits, CreditsCtx};
use crate::services::Services;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Generate a CREDITS.md from attribution sidecars/manifests.
#[derive(Debug, Args)]
pub struct CreditsCmd {
    /// Project root to scan (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// Output file path (defaults to `<root>/CREDITS.md`).
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Run the credits command.
///
/// # Errors
///
/// Returns an error if services, config load, or credit generation fail.
pub fn run(cmd: &CreditsCmd) -> Result<CommandStatus, Report<AppError>> {
    let root = &cmd.root;
    let services = Services::real().change_context(AppError)?;
    let config = Config::load(&services.fs, root)
        .change_context(AppError)
        .attach("failed to load config")?;
    let output = cmd
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(root));
    let ctx = CreditsCtx {
        services: &services,
        config: &config,
        root,
    };
    generate_credits(&ctx, &output)
        .change_context(AppError)
        .attach("failed to generate credits")?;
    println!("credits: wrote {}", output.display());
    Ok(CommandStatus::Success)
}
