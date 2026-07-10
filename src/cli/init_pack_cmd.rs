//! `auditah init-pack` — write a directory manifest.toml.

use std::path::PathBuf;

use auditah::model::terms::Overrides;
use auditah::AppError;
use clap::Args;

use auditah::add::write_manifest;
use auditah::model::attribution::AttributionRecord;
use auditah::services::Services;
use error_stack::{Report, ResultExt};

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

/// Run the init-pack command. Returns the process exit code.
pub fn run(cmd: &InitPackCmd) -> Result<(), Report<AppError>> {
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
    let services = Services::real().change_context(AppError)?;
    write_manifest(&services, &cmd.dir, &record).change_context(AppError)?;
    println!("init-pack: wrote {}/manifest.toml", cmd.dir.display());
    Ok(())
}
