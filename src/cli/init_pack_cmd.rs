//! `auditah init-pack` — write a directory manifest.toml.

use std::path::PathBuf;

use crate::model::terms::Overrides;
use crate::AppError;
use clap::Args;

use crate::add::write_manifest;
use crate::model::attribution::AttributionRecord;
use crate::services::Services;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Write a `manifest.toml` covering a directory + its subdirs.
#[derive(Debug, Args)]
pub struct InitPackCmd {
    /// Directory to cover with a manifest.
    pub dir: PathBuf,

    /// SPDX license ID (e.g. CC0-1.0, CC-BY-3.0).
    #[arg(long)]
    pub license: String,

    /// Author / copyright holder.
    #[arg(long)]
    pub author: String,

    /// Copyright year.
    #[arg(long)]
    pub year: Option<u16>,

    /// Title prefix (defaults to the directory name).
    #[arg(long)]
    pub title: Option<String>,

    /// Source URL.
    #[arg(long)]
    pub source: Option<String>,
}

/// Run the init-pack command.
///
/// # Errors
///
/// Returns an error if services fail or the manifest write fails.
pub fn run(cmd: &InitPackCmd) -> Result<CommandStatus, Report<AppError>> {
    let title = cmd
        .title
        .clone()
        .or_else(|| {
            cmd.dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let record = AttributionRecord {
        title,
        author: cmd.author.clone(),
        year: cmd.year.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(2025, |d| {
                    u16::try_from(d.as_secs() / 31_557_600).unwrap_or(2025)
                })
        }),
        license: cmd.license.clone(),
        source: cmd.source.clone().unwrap_or_default(),
        modified: false,
        package: None,
        overrides: Overrides::default(),
    };
    let services = Services::real(&cmd.dir).change_context(AppError)?;
    write_manifest(&services, &cmd.dir, &record).change_context(AppError)?;
    println!("init-pack: wrote {}/manifest.toml", cmd.dir.display());
    Ok(CommandStatus::Success)
}
