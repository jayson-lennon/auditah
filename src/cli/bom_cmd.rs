//! `auditah bom` — generate a license bill of materials (BOM.md).

use std::path::PathBuf;

use crate::AppError;
use clap::Args;

use crate::bom::{default_output_path, generate_bom, BomCtx};
use crate::config::Config;
use crate::services::Services;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Generate a license bill of materials (BOM.md) from attribution data.
#[derive(Debug, Args)]
pub struct BomCmd {
    /// Project root to scan (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// Output file path (defaults to `<root>/BOM.md`).
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Run the bom command.
///
/// # Errors
///
/// Returns an error if services, config load, or BOM generation fail.
pub fn run(cmd: &BomCmd) -> Result<CommandStatus, Report<AppError>> {
    let root = &cmd.root;
    let services = Services::real(root).change_context(AppError)?;
    let config = Config::load(&services.fs, root)
        .change_context(AppError)
        .attach("failed to load config")?;
    let output = cmd
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(root));
    let ctx = BomCtx {
        services: &services,
        config: &config,
        root,
    };
    generate_bom(&ctx, &output)
        .change_context(AppError)
        .attach("failed to generate BOM")?;
    println!("bom: wrote {}", output.display());
    Ok(CommandStatus::Success)
}
