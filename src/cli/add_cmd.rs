//! `auditah add` — scaffold a sidecar for a single asset.

use std::io::{self, Write};
use std::path::PathBuf;

use clap::Args;

use auditah::add::write_sidecar;
use auditah::model::attribution::AttributionRecord;
use auditah::services::Services;

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
/// Run the add command. Returns the process exit code.
pub fn run(cmd: &AddCmd) -> i32 {
    let record = match build_record(cmd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let services = Services::real();
    match write_sidecar(&services, &cmd.file, &record) {
        Ok(()) => {
            println!("add: wrote {}.attr.toml", cmd.file.display());
            0
        }
        Err(e) => {
            eprintln!("error: {e:?}");
            2
        }
    }
}

/// Prompt interactively for any missing field. Flags provided on the CLI skip
/// the prompt for that field.
fn build_record(cmd: &AddCmd) -> Result<AttributionRecord, String> {
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
        overrides: Default::default(),
    })
}
/// Read one string field, prompting if `value` is None.
fn field(value: Option<String>, prompt: &str) -> Result<String, String> {
    if let Some(v) = value {
        return Ok(v);
    }
    print!("{prompt}: ");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let trimmed = line.trim().to_string();
    if trimmed.is_empty() {
        return Err(format!("{prompt} is required"));
    }
    Ok(trimmed)
}

/// Read the year, prompting if missing.
fn field_year(year: Option<u16>) -> Result<u16, String> {
    if let Some(y) = year {
        return Ok(y);
    }
    let raw = field(None, "Copyright year")?;
    raw.parse::<u16>()
        .map_err(|_| "year must be a number".to_string())
}
