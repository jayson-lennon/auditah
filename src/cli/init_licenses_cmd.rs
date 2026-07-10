//! `auditah init-licenses` — write full license text files to `LICENSES/`.

use std::path::PathBuf;

use clap::Args;

use auditah::services::Services;
use auditah::{init_licenses::init_licenses, AppError};
use error_stack::{Report, ResultExt};

/// Write `LICENSES/<id>.txt` for every license in the registry.
///
/// Idempotent: existing files with matching content are skipped. Divergent
/// files (human-edited) cause an error — on-disk text is authoritative, so we
/// never clobber silently.
#[derive(Debug, Args)]
pub struct InitLicensesCmd {
    /// Project root to write `LICENSES/` into (defaults to the current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Run the init-licenses command. Returns the process exit code.
pub fn run(cmd: &InitLicensesCmd) -> Result<(), Report<AppError>> {
    let services = Services::real().change_context(AppError)?;
    let outcome = init_licenses(&services, &cmd.root).change_context(AppError)?;
    println!(
        "init-licenses: wrote {} license text file(s) to {}/LICENSES (skipped {} already present)",
        outcome.written,
        cmd.root.display(),
        outcome.skipped,
    );
    Ok(())
}
