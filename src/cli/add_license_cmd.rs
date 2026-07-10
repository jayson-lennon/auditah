//! `auditah add-license` — scaffold a license grid in `LICENSES/`.

use std::path::PathBuf;

use crate::add_license::write_license_template;
use crate::services::Services;
use crate::AppError;
use clap::Args;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Scaffold a `<root>/LICENSES/LicenseRef-<name>.toml` license grid with
/// permissive defaults and a comment on every field. Non-interactive; refuses
/// to overwrite an existing file.
#[derive(Debug, Args)]
pub struct AddLicenseCmd {
    /// License name. Prefixed with `LicenseRef-` if not already (e.g. `Foo`
    /// becomes `LicenseRef-Foo`).
    pub name: String,

    /// Project root containing `LICENSES/`. Defaults to the current directory.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Run the add-license command.
///
/// # Errors
///
/// Returns an error if services fail or the template write fails (e.g. the file
/// already exists, or the write fails).
pub fn run(cmd: &AddLicenseCmd) -> Result<CommandStatus, Report<AppError>> {
    let services = Services::real(&cmd.root).change_context(AppError)?;
    let path = write_license_template(&services, &cmd.root, &cmd.name).change_context(AppError)?;
    println!("add-license: wrote {}", path.display());
    Ok(CommandStatus::Success)
}
