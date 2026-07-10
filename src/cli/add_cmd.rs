//! `auditah add` — scaffold a sidecar for a single asset.

use std::io::{self, Write};
use std::path::PathBuf;

use crate::model::terms::Overrides;
use crate::AppError;
use clap::Args;

use crate::add::write_sidecar;
use crate::model::attribution::AttributionRecord;
use crate::services::Services;
use error_stack::{Report, ResultExt};
use wherror::Error;

use super::CommandStatus;

/// Scaffold an `<asset>.attr.toml` sidecar for a single asset.
#[derive(Debug, Args)]
pub struct AddCmd {
    /// Path to the asset file to annotate.
    pub file: PathBuf,
    /// Title of the work.
    #[arg(long)]
    pub title: Option<String>,
    /// Author / copyright holder.
    #[arg(long)]
    pub author: Option<String>,
    /// Copyright year.
    #[arg(long)]
    pub year: Option<u16>,
    /// SPDX license id (e.g. CC-BY-3.0, CC0-1.0).
    #[arg(long)]
    pub license: Option<String>,
    /// Source URL.
    #[arg(long)]
    pub source: Option<String>,
    /// Whether the asset has been modified.
    #[arg(long)]
    pub modified: bool,
}
/// Run the add command.
///
/// # Errors
///
/// Returns an error if field prompting, services, or the sidecar write fail.
pub fn run(cmd: &AddCmd) -> Result<CommandStatus, Report<AppError>> {
    let record = build_record(cmd).change_context(AppError)?;
    let services = Services::real().change_context(AppError)?;
    write_sidecar(&services, &cmd.file, &record).change_context(AppError)?;
    println!("add: wrote {}.attr.toml", cmd.file.display());
    Ok(CommandStatus::Success)
}

/// Prompt interactively for any missing field. Flags provided on the CLI skip
/// the prompt for that field.
fn build_record(cmd: &AddCmd) -> Result<AttributionRecord, Report<FieldError>> {
    let title = field(cmd.title.clone(), "Title")?;
    let author = field(cmd.author.clone(), "Author")?;
    let year = field_year(cmd.year)?;
    let license = field(cmd.license.clone(), "License id (e.g. CC-BY-3.0)")?;
    let source = field(cmd.source.clone(), "Source URL")?;
    Ok(AttributionRecord {
        title,
        author,
        year,
        license,
        source,
        modified: cmd.modified,
        package: None,
        overrides: Overrides::default(),
    })
}

/// A field processing error.
#[derive(Debug, Error)]
#[error(debug)]
pub struct FieldError;

/// Read one string field, prompting if `value` is None.
fn field(value: Option<String>, prompt: &str) -> Result<String, Report<FieldError>> {
    if let Some(v) = value {
        return Ok(v);
    }
    print!("{prompt}: ");
    io::stdout().flush().change_context(FieldError)?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .change_context(FieldError)?;
    let trimmed = line.trim().to_string();
    if trimmed.is_empty() {
        return Err(Report::from(FieldError).attach(format!("{prompt} is required")));
    }
    Ok(trimmed)
}

/// Read the year, prompting if missing.
fn field_year(year: Option<u16>) -> Result<u16, Report<FieldError>> {
    if let Some(y) = year {
        return Ok(y);
    }
    let raw = field(None, "Copyright year")?;
    raw.parse::<u16>()
        .change_context(FieldError)
        .attach("year must be a number")
}
